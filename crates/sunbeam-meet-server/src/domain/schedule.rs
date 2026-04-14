//! Schedule (scheduled meeting) domain constants + RRULE parser.

use thiserror::Error;

/// Invitee response values.
pub const RESPONSE_PENDING: &str = "pending";
/// Accepted.
pub const RESPONSE_ACCEPTED: &str = "accepted";
/// Declined.
pub const RESPONSE_DECLINED: &str = "declined";
/// Tentative.
pub const RESPONSE_TENTATIVE: &str = "tentative";

/// RRULE recurrence frequency. Mirrors iCalendar RFC 5545 FREQ.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Frequency {
    /// FREQ=DAILY
    Daily,
    /// FREQ=WEEKLY
    Weekly,
    /// FREQ=MONTHLY
    Monthly,
    /// FREQ=YEARLY
    Yearly,
}

/// Weekday in a BYDAY entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Weekday {
    /// Monday.
    Mo,
    /// Tuesday.
    Tu,
    /// Wednesday.
    We,
    /// Thursday.
    Th,
    /// Friday.
    Fr,
    /// Saturday.
    Sa,
    /// Sunday.
    Su,
}

/// Parsed RRULE.
#[derive(Debug, Clone)]
pub struct Recurrence {
    /// FREQ.
    pub frequency: Frequency,
    /// INTERVAL, default 1.
    pub interval: u32,
    /// COUNT, if present.
    pub count: Option<u32>,
    /// BYDAY, if present.
    pub by_weekday: Vec<Weekday>,
}

/// Errors raised by [`parse_rrule`].
#[derive(Debug, Error)]
pub enum RRuleError {
    /// Input was empty.
    #[error("rrule must not be empty")]
    Empty,
    /// Mandatory FREQ missing.
    #[error("rrule missing FREQ")]
    MissingFreq,
    /// FREQ value was not one of the supported frequencies.
    #[error("unknown FREQ: {0}")]
    UnknownFrequency(String),
    /// A key=value pair could not be parsed.
    #[error("malformed rrule pair: {0}")]
    MalformedPair(String),
    /// INTERVAL / COUNT / BYDAY parse error.
    #[error("invalid value for {key}: {value}")]
    InvalidValue {
        /// Key whose value failed to parse.
        key: String,
        /// Offending value.
        value: String,
    },
}

/// Parse an iCalendar RRULE string like `FREQ=WEEKLY;BYDAY=MO,WE,FR` into a
/// [`Recurrence`]. Missing fields take documented defaults.
pub fn parse_rrule(s: &str) -> Result<Recurrence, RRuleError> {
    if s.is_empty() {
        return Err(RRuleError::Empty);
    }
    let mut freq: Option<Frequency> = None;
    let mut interval: u32 = 1;
    let mut count: Option<u32> = None;
    let mut by_weekday: Vec<Weekday> = Vec::new();

    for part in s.split(';').filter(|p| !p.is_empty()) {
        let Some((key, value)) = part.split_once('=') else {
            return Err(RRuleError::MalformedPair(part.to_owned()));
        };
        match key {
            "FREQ" => {
                freq = Some(match value {
                    "DAILY" => Frequency::Daily,
                    "WEEKLY" => Frequency::Weekly,
                    "MONTHLY" => Frequency::Monthly,
                    "YEARLY" => Frequency::Yearly,
                    other => return Err(RRuleError::UnknownFrequency(other.to_owned())),
                });
            }
            "INTERVAL" => {
                interval = value.parse().map_err(|_| RRuleError::InvalidValue {
                    key: key.to_owned(),
                    value: value.to_owned(),
                })?;
            }
            "COUNT" => {
                count = Some(value.parse().map_err(|_| RRuleError::InvalidValue {
                    key: key.to_owned(),
                    value: value.to_owned(),
                })?);
            }
            "BYDAY" => {
                for day in value.split(',') {
                    let wd = match day {
                        "MO" => Weekday::Mo,
                        "TU" => Weekday::Tu,
                        "WE" => Weekday::We,
                        "TH" => Weekday::Th,
                        "FR" => Weekday::Fr,
                        "SA" => Weekday::Sa,
                        "SU" => Weekday::Su,
                        other => {
                            return Err(RRuleError::InvalidValue {
                                key: "BYDAY".into(),
                                value: other.to_owned(),
                            });
                        }
                    };
                    by_weekday.push(wd);
                }
            }
            // Unknown keys are ignored — RFC 5545 RRULE is extensible.
            _ => {}
        }
    }

    let frequency = freq.ok_or(RRuleError::MissingFreq)?;
    Ok(Recurrence {
        frequency,
        interval,
        count,
        by_weekday,
    })
}
