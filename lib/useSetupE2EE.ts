import { decodePassphrase } from './client-utils';

export function useSetupE2EE(serverPassphrase?: string) {
  const e2eePassphrase =
    serverPassphrase ||
    (typeof window !== 'undefined' ? decodePassphrase(location.hash.substring(1)) : undefined);

  const worker: Worker | undefined =
    typeof window !== 'undefined' && e2eePassphrase
      ? new Worker(new URL('livekit-client/e2ee-worker', import.meta.url))
      : undefined;

  return { worker, e2eePassphrase };
}
