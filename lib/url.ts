// SPDX-License-Identifier: AGPL-3.0-or-later
export function getMeetBaseUrl(): string {
  const base = process.env.MEET_BASE_URL;
  if (base) {
    return base.replace(/\/+$/, '');
  }
  return '';
}

export function buildMeetingUrl(roomName: string, role: 'host' | 'guest' = 'host'): string {
  const base = getMeetBaseUrl();
  if (!base) {
    return `/rooms/${encodeURIComponent(roomName)}?role=${role}`;
  }
  return `${base}/rooms/${encodeURIComponent(roomName)}?role=${role}`;
}
