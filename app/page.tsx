import { auth, signIn } from '@/auth';
import styles from '../styles/Home.module.css';

export default async function Page() {
  const session = await auth();

  if (session?.user) {
    return (
      <main className={styles.main} data-lk-theme="default">
        <div className="header">
          <h1>Video Calls</h1>
          <p>Signed in as {session.user.email ?? session.user.name ?? 'Unknown'}</p>
          <p>Join a meeting from your calendar invitation.</p>
        </div>
      </main>
    );
  }

  return (
    <main className={styles.main} data-lk-theme="default">
      <div className="header">
        <h1>Video Calls</h1>
        <p>Sign in to join a meeting.</p>
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
