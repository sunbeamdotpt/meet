/**
 * LiveKit Meet plugin for Bulwark Mail.
 *
 * Adds an "Add LiveKit Meeting" button to the calendar event editor.
 * The button calls the livekit-meet app (proxied behind the same origin)
 * which verifies the user's OIDC token, creates a LiveKit room, and
 * returns the organizer URL.
 */

const { createElement: h, useState } = require('react');
const slotApi = require('@plugin-host');

function LiveKitAddButton(props) {
  const [busy, setBusy] = useState(false);
  const eventData = props?.eventData || {};
  const eventTitle = eventData.title || 'Meeting';
  const eventUid = eventData.uid || undefined;
  const startTime = eventData.startTime || undefined;
  const endTime = eventData.endTime || undefined;
  const setVirtualLocation = props?.setVirtualLocation;

  async function handleClick() {
    if (busy) return;
    setBusy(true);

    try {
      const payload = {
        eventTitle,
        ...(eventUid ? { eventUid } : {}),
        ...(startTime ? { startTime } : {}),
        ...(endTime ? { endTime } : {}),
      };

      const data = await slotApi.http.post('/api/bulwark/rooms', payload);
      const url = data?.url;
      if (!url) throw new Error('No url in response');

      if (typeof setVirtualLocation === 'function') {
        await setVirtualLocation(url);
        slotApi.toast.success('LiveKit meeting added to event');
      } else {
        await slotApi.ui.alert({
          title: 'LiveKit meeting created',
          message: url,
        });
      }
    } catch (err) {
      const message = err && err.message ? err.message : String(err);
      slotApi.toast.error(`Could not create LiveKit meeting: ${message}`);
    } finally {
      setBusy(false);
    }
  }

  return h(
    'button',
    {
      type: 'button',
      onClick: handleClick,
      disabled: busy,
      style: {
        font: 'inherit',
        padding: '6px 10px',
        borderRadius: '6px',
        border: '1px solid var(--color-input)',
        background: 'var(--color-muted)',
        color: 'var(--color-foreground)',
        cursor: busy ? 'progress' : 'pointer',
      },
    },
    busy ? 'Generating…' : '📹 Add LiveKit Meeting',
  );
}

export const slots = {
  'calendar-event-actions': {
    component: LiveKitAddButton,
    order: 10,
  },
};

export async function activate(api) {
  api.log.info('LiveKit Meet plugin activated');
}
