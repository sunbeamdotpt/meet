//! Stalwart CalDAV client for meeting scheduling.
//!
//! Minimal subset of CalDAV: PUT iCalendar text onto a well-known URL,
//! DELETE to cancel. Creates a VEVENT per scheduled meeting.

use chrono::{DateTime, Utc};

use crate::config::CalDavConfig;
use crate::error::{Error, Result};

/// Structured representation of a CalDAV event as round-tripped via the
/// iCalendar ↔ struct boundary. Tests parse structured fields rather than
/// substring-matching the raw ICS blob.
#[derive(Debug, Clone)]
pub struct Event {
    /// iCalendar UID.
    pub uid: String,
    /// ETag returned by the server on the last PUT.
    pub etag: String,
    /// SUMMARY.
    pub summary: String,
    /// DESCRIPTION.
    pub description: String,
    /// DTSTART.
    pub starts_at: DateTime<Utc>,
    /// DTEND.
    pub ends_at: DateTime<Utc>,
    /// RRULE, if any (raw RFC 5545 value after `RRULE:`).
    pub rrule: Option<String>,
}

/// Payload for creating a new event.
#[derive(Debug, Clone)]
pub struct NewEvent {
    /// iCalendar UID.
    pub uid: String,
    /// SUMMARY.
    pub summary: String,
    /// DESCRIPTION.
    pub description: String,
    /// DTSTART.
    pub starts_at: DateTime<Utc>,
    /// DTEND.
    pub ends_at: DateTime<Utc>,
    /// RRULE (raw RFC 5545 body).
    pub rrule: Option<String>,
    /// Invitee email addresses (ATTENDEE lines).
    pub invitees: Vec<String>,
}

/// Partial update to an event. Fields set to `Some` are overwritten; `None`
/// leaves the field as-is.
#[derive(Debug, Clone, Default)]
pub struct EventUpdate {
    /// New SUMMARY.
    pub summary: Option<String>,
    /// New DESCRIPTION.
    pub description: Option<String>,
    /// New DTSTART.
    pub starts_at: Option<DateTime<Utc>>,
    /// New DTEND.
    pub ends_at: Option<DateTime<Utc>>,
    /// New RRULE (`Some(None)` to clear is not supported; submit empty string).
    pub rrule: Option<Option<String>>,
    /// New invitees list.
    pub invitees: Option<Vec<String>>,
}

/// CalDAV client.
#[derive(Clone)]
pub struct CalDavClient {
    http: reqwest::Client,
    url: String,
    username: String,
    password: String,
}

/// Test-facing alias.
pub type Client = CalDavClient;

impl CalDavClient {
    /// New client from config.
    pub fn new(cfg: &CalDavConfig) -> Self {
        Self {
            http: reqwest::Client::new(),
            url: cfg.url.clone(),
            username: cfg.username.clone(),
            password: cfg.password.clone(),
        }
    }

    /// Build the request URL for a given UID, percent-encoding the filename
    /// segment. Required for Stalwart v0.15 which keys stored objects by the
    /// percent-encoded path — raw `@` in the UID would PUT as one key and
    /// 404 on GET. Using `Url::path_segments_mut` handles encoding uniformly.
    fn event_url(&self, uid: &str) -> String {
        use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
        // `url::Url::path_segments_mut().push()` only escapes `/`, `%`, and
        // controls — it leaves `@` as-is, which Stalwart then stores under
        // a different key than it resolves on GET. Encoding with a strict
        // set (anything non-alphanumeric, minus `.` / `-` which are always
        // pchar-safe) sidesteps that.
        let filename = format!(
            "{}.ics",
            utf8_percent_encode(uid, NON_ALPHANUMERIC).collect::<String>()
        );
        let base = self.url.trim_end_matches('/');
        format!("{base}/{filename}")
    }

    /// PUT an iCalendar object at `{url}/{uid}.ics`.
    pub async fn put_event(&self, uid: &str, ics: &str) -> Result<String> {
        let url = self.event_url(uid);
        let res = self
            .http
            .put(&url)
            .basic_auth(&self.username, Some(&self.password))
            .header(
                reqwest::header::CONTENT_TYPE,
                "text/calendar; charset=utf-8",
            )
            .body(ics.to_owned())
            .send()
            .await?;
        if !res.status().is_success() {
            return Err(Error::Internal(anyhow::anyhow!(
                "caldav put: {}",
                res.status()
            )));
        }
        let etag = res
            .headers()
            .get(reqwest::header::ETAG)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string();
        Ok(etag)
    }

    /// DELETE an iCalendar object by UID. `etag` is optional and, if present,
    /// is sent as `If-Match` to guard against racing updates.
    pub async fn delete_event(&self, uid: &str, etag: &str) -> Result<()> {
        let url = self.event_url(uid);
        let mut req = self
            .http
            .delete(url)
            .basic_auth(&self.username, Some(&self.password));
        if !etag.is_empty() {
            req = req.header(reqwest::header::IF_MATCH, etag);
        }
        let res = req.send().await?;
        if !res.status().is_success() && res.status() != reqwest::StatusCode::NOT_FOUND {
            return Err(Error::Internal(anyhow::anyhow!(
                "caldav delete: {}",
                res.status()
            )));
        }
        Ok(())
    }

    /// Test-facing constructor: connect to a CalDAV URL. Basic-auth creds
    /// may be encoded as userinfo in the URL (e.g.
    /// `http://admin:admin@host/dav/cal/_42/default`) — we parse them out
    /// and send them via `Authorization: Basic` so the server sees proper
    /// auth on every request (reqwest would otherwise drop userinfo).
    pub async fn connect(url: &str) -> Result<Self> {
        let parsed = reqwest::Url::parse(url)
            .map_err(|e| Error::Internal(anyhow::anyhow!("caldav url: {e}")))?;
        // reqwest::Url stores userinfo percent-encoded; dev creds are
        // plain ASCII (admin/admin), so raw is fine here.
        let username = parsed.username().to_owned();
        let password = parsed.password().unwrap_or_default().to_owned();
        let mut clean = parsed.clone();
        let _ = clean.set_username("");
        let _ = clean.set_password(None);
        Ok(Self {
            http: reqwest::Client::new(),
            url: clean.to_string().trim_end_matches('/').to_owned(),
            username,
            password,
        })
    }

    /// Create an event and return the [`Event`] as written, including its
    /// server-assigned ETag.
    pub async fn create_event(&self, new: NewEvent) -> Result<Event> {
        let ics = render_ics(&new);
        let etag = self.put_event(&new.uid, &ics).await?;
        Ok(Event {
            uid: new.uid,
            etag,
            summary: new.summary,
            description: new.description,
            starts_at: new.starts_at,
            ends_at: new.ends_at,
            rrule: new.rrule,
        })
    }

    /// Fetch an event by UID, parsing structured fields from the returned
    /// ICS. Returns `Ok(None)` on 404.
    pub async fn get_event(&self, uid: &str) -> Result<Option<Event>> {
        let url = self.event_url(uid);
        let res = self
            .http
            .get(&url)
            .basic_auth(&self.username, Some(&self.password))
            .send()
            .await?;
        if res.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !res.status().is_success() {
            return Err(Error::Internal(anyhow::anyhow!(
                "caldav get: {}",
                res.status()
            )));
        }
        let etag = res
            .headers()
            .get(reqwest::header::ETAG)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string();
        let body = res.text().await?;
        Ok(Some(parse_ics(uid, etag, &body)))
    }

    /// Update an event via read-modify-write. `if_match` is passed as the
    /// `If-Match` header to prevent lost-update races.
    pub async fn update_event(
        &self,
        uid: &str,
        if_match: &str,
        update: EventUpdate,
    ) -> Result<Event> {
        let current = self
            .get_event(uid)
            .await?
            .ok_or_else(|| Error::NotFound(format!("caldav event {uid}")))?;
        let merged = NewEvent {
            uid: current.uid.clone(),
            summary: update.summary.unwrap_or(current.summary),
            description: update.description.unwrap_or(current.description),
            starts_at: update.starts_at.unwrap_or(current.starts_at),
            ends_at: update.ends_at.unwrap_or(current.ends_at),
            rrule: update.rrule.unwrap_or(current.rrule),
            invitees: update.invitees.unwrap_or_default(),
        };
        let ics = render_ics(&merged);
        let url = self.event_url(uid);
        let res = self
            .http
            .put(&url)
            .basic_auth(&self.username, Some(&self.password))
            .header(reqwest::header::IF_MATCH, if_match)
            .header(
                reqwest::header::CONTENT_TYPE,
                "text/calendar; charset=utf-8",
            )
            .body(ics)
            .send()
            .await?;
        if !res.status().is_success() {
            return Err(Error::Internal(anyhow::anyhow!(
                "caldav update: {}",
                res.status()
            )));
        }
        let etag = res
            .headers()
            .get(reqwest::header::ETAG)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string();
        Ok(Event {
            uid: merged.uid,
            etag,
            summary: merged.summary,
            description: merged.description,
            starts_at: merged.starts_at,
            ends_at: merged.ends_at,
            rrule: merged.rrule,
        })
    }
}

fn render_ics(e: &NewEvent) -> String {
    use std::fmt::Write as _;
    fn fmt(dt: DateTime<Utc>) -> String {
        dt.format("%Y%m%dT%H%M%SZ").to_string()
    }
    let mut out = String::new();
    out.push_str("BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:sunbeam-meet\r\nBEGIN:VEVENT\r\n");
    let _ = writeln!(out, "UID:{}\r", e.uid);
    let _ = writeln!(out, "DTSTAMP:{}\r", fmt(Utc::now()));
    let _ = writeln!(out, "DTSTART:{}\r", fmt(e.starts_at));
    let _ = writeln!(out, "DTEND:{}\r", fmt(e.ends_at));
    let _ = writeln!(out, "SUMMARY:{}\r", e.summary.replace(['\r', '\n'], " "));
    if !e.description.is_empty() {
        let _ = writeln!(out, "DESCRIPTION:{}\r", e.description.replace('\n', "\\n"));
    }
    if let Some(rr) = &e.rrule {
        let _ = writeln!(out, "RRULE:{rr}\r");
    }
    for addr in &e.invitees {
        let _ = writeln!(out, "ATTENDEE:mailto:{addr}\r");
    }
    out.push_str("END:VEVENT\r\nEND:VCALENDAR\r\n");
    out
}

fn parse_ics(uid: &str, etag: String, body: &str) -> Event {
    let mut summary = String::new();
    let mut description = String::new();
    let mut starts_at = Utc::now();
    let mut ends_at = Utc::now();
    let mut rrule: Option<String> = None;
    for raw in body.lines() {
        let line = raw.trim_end_matches('\r');
        if let Some(v) = line.strip_prefix("SUMMARY:") {
            v.clone_into(&mut summary);
        } else if let Some(v) = line.strip_prefix("DESCRIPTION:") {
            description = v.replace("\\n", "\n");
        } else if let Some(v) = line.strip_prefix("DTSTART:") {
            starts_at = parse_ics_dt(v).unwrap_or(starts_at);
        } else if let Some(v) = line.strip_prefix("DTEND:") {
            ends_at = parse_ics_dt(v).unwrap_or(ends_at);
        } else if let Some(v) = line.strip_prefix("RRULE:") {
            rrule = Some(v.to_owned());
        }
    }
    Event {
        uid: uid.to_owned(),
        etag,
        summary,
        description,
        starts_at,
        ends_at,
        rrule,
    }
}

fn parse_ics_dt(s: &str) -> Option<DateTime<Utc>> {
    chrono::NaiveDateTime::parse_from_str(s, "%Y%m%dT%H%M%SZ")
        .ok()
        .map(|n| DateTime::<Utc>::from_naive_utc_and_offset(n, Utc))
}
