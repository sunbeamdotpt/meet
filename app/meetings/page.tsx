// SPDX-License-Identifier: AGPL-3.0-or-later
import { auth, signIn } from '@/auth';
import { getUpcomingMeetings, Meeting } from '@/lib/db';
import styles from '../../styles/Home.module.css';

function formatTime(start: Date | string | null, end: Date | string | null): string {
  if (!start) return 'No scheduled time';

  const startDate = new Date(start);
  const endDate = end ? new Date(end) : null;

  const dateFormatter = new Intl.DateTimeFormat(undefined, {
    weekday: 'short',
    month: 'short',
    day: 'numeric',
    hour: 'numeric',
    minute: '2-digit',
  });

  if (endDate) {
    const timeFormatter = new Intl.DateTimeFormat(undefined, {
      hour: 'numeric',
      minute: '2-digit',
    });
    return `${dateFormatter.format(startDate)} – ${timeFormatter.format(endDate)}`;
  }

  return dateFormatter.format(startDate);
}

function MeetingCard({ meeting }: { meeting: Meeting }) {
  return (
    <div className={styles.meetingCard}>
      <div>
        <h3>{meeting.event_title}</h3>
        <p className={styles.meetingTime}>{formatTime(meeting.start_time, meeting.end_time)}</p>
      </div>
      <a className="lk-button" href={meeting.url}>
        Join
      </a>
    </div>
  );
}

export default async function MeetingsPage() {
  const session = await auth();

  if (!session?.user?.email) {
    return (
      <main className={styles.main} data-lk-theme="default">
        <div className="header">
          <h1>Upcoming meetings</h1>
          <p>Sign in to see your meetings.</p>
        </div>
        <form
          className={styles.tabContent}
          action={async () => {
            'use server';
            await signIn('oidc');
          }}
        >
          <button
            style={{ paddingInline: '1.25rem', width: '100%' }}
            className="lk-button"
            type="submit"
          >
            Sign in
          </button>
        </form>
      </main>
    );
  }

  let meetings: Meeting[] = [];
  let error: string | null = null;

  try {
    meetings = await getUpcomingMeetings(session.user.email);
  } catch (err) {
    if (err instanceof Error) {
      error = err.message;
    } else {
      error = 'Failed to load meetings';
    }
  }

  return (
    <main className={styles.main} data-lk-theme="default">
      <div className="header">
        <h1>Upcoming meetings</h1>
        <p>Signed in as {session.user.email}</p>
      </div>

      {error && <p className={styles.error}>{error}</p>}

      <div className={styles.tabContent}>
        {meetings.length === 0 ? (
          <p>No upcoming meetings.</p>
        ) : (
          <div className={styles.meetingList}>
            {meetings.map((meeting) => (
              <MeetingCard key={meeting.id} meeting={meeting} />
            ))}
          </div>
        )}
      </div>
    </main>
  );
}
