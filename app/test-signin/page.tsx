'use client';

import { useSearchParams } from 'next/navigation';
import { signIn } from 'next-auth/react';
import React, { Suspense } from 'react';

function TestSignIn() {
  const searchParams = useSearchParams();
  const [error, setError] = React.useState<string | null>(null);

  React.useEffect(() => {
    const name = searchParams.get('name') ?? 'Test User';
    const email = searchParams.get('email') ?? 'test@example.com';
    const callbackUrl = searchParams.get('callbackUrl') ?? '/';

    signIn('test', { name, email, redirect: false })
      .then(async (result) => {
        if (result?.ok) {
          // Wait until the session cookie is actually persisted before
          // navigating to the room, avoiding a flaky auth check on the room page.
          for (let i = 0; i < 30; i++) {
            try {
              const res = await fetch('/api/auth/session');
              const data = (await res.json()) as { user?: { email?: string } };
              if (data.user?.email) {
                window.location.assign(callbackUrl);
                return;
              }
            } catch {
              // ignore
            }
            await new Promise((resolve) => window.setTimeout(resolve, 100));
          }
          setError('Session was not established in time');
        } else {
          setError(result?.error ?? 'Sign in failed');
        }
      })
      .catch((err) => {
        setError(err instanceof Error ? err.message : String(err));
      });
  }, [searchParams]);

  if (error) {
    return (
      <main data-lk-theme="default" style={{ padding: '2rem' }}>
        <h1>Test sign-in failed</h1>
        <p>{error}</p>
      </main>
    );
  }

  return (
    <main data-lk-theme="default" style={{ padding: '2rem' }}>
      <h1>Signing in…</h1>
    </main>
  );
}

export default function TestSignInPage() {
  return (
    <Suspense
      fallback={
        <main data-lk-theme="default" style={{ padding: '2rem' }}>
          <h1>Signing in…</h1>
        </main>
      }
    >
      <TestSignIn />
    </Suspense>
  );
}
