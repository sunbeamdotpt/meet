/**
 * LiveKit Meet plugin for Bulwark Mail.
 *
 * Adds an "Add LiveKit Meeting" button to the calendar event editor.
 * The button calls the livekit-meet app (proxied behind the same origin)
 * which verifies the user's OIDC token, creates a LiveKit room, and
 * returns the organizer URL.
 */

const { createElement: h, useState } = require('react');
const api = require('@plugin-host');

function getSetting(key, fallback) {
  const settings = api?.plugin?.settings || {};
  return settings[key] !== undefined ? settings[key] : fallback;
}

function LiveKitAddButton(props) {
  const [busy, setBusy] = useState(false);
  const eventData = props?.eventData || {};
  const setVirtualLocation = props?.setVirtualLocation;

  const buttonLabel = getSetting('buttonLabel', 'Add LiveKit Meeting');
  const autoSetLocation = getSetting('autoSetLocation', true);
  const apiPath = getSetting('apiPath', '/api/bulwark/rooms');

  async function handleClick() {
    if (busy) return;
    setBusy(true);

    try {
      const payload = {
        eventTitle: eventData.title || 'Meeting',
        ...(eventData.uid ? { eventUid: eventData.uid } : {}),
        ...(eventData.start ? { startTime: eventData.start } : {}),
        ...(eventData.end ? { endTime: eventData.end } : {}),
      };

      // The host wraps same-origin POSTs in { ok, status, data }.
      const response = await api.http.post(apiPath, payload);
      const body = response?.data ?? response;
      const url = body?.url;
      if (!url) throw new Error('No meeting URL returned');

      if (autoSetLocation && typeof setVirtualLocation === 'function') {
        await setVirtualLocation(url);
        api.toast.success('LiveKit meeting added to event');
      } else {
        await api.ui.alert({
          title: 'LiveKit meeting created',
          message: url,
        });
      }
    } catch (err) {
      const message = err && err.message ? err.message : String(err);
      api.toast.error(`Could not create LiveKit meeting: ${message}`);
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
    busy ? 'Generating…' : `📹 ${buttonLabel}`,
  );
}

export const slots = {
  'calendar-event-actions': {
    component: LiveKitAddButton,
    order: 10,
  },
};

export async function activate(pluginApi) {
  pluginApi.log.info('LiveKit Meet plugin activated');
}
