'use client';
import * as React from 'react';
import { Track } from 'livekit-client';
import {
  useMaybeLayoutContext,
  MediaDeviceMenu,
  TrackToggle,
  useRoomContext,
  useIsRecording,
} from '@livekit/components-react';
import styles from '../styles/SettingsMenu.module.css';
import { CameraSettings } from './CameraSettings';
import { MicrophoneSettings } from './MicrophoneSettings';

// Recording is enabled by default for this deployment.
const RECORDING_ENABLED = true;

/**
 * @alpha
 */
export interface SettingsMenuProps extends React.HTMLAttributes<HTMLDivElement> {}

/**
 * @alpha
 */
export function SettingsMenu(props: SettingsMenuProps) {
  const layoutContext = useMaybeLayoutContext();
  const room = useRoomContext();
  const isRecording = useIsRecording();
  const [egressId, setEgressId] = React.useState<string | null>(null);
  const [processingRecRequest, setProcessingRecRequest] = React.useState(false);
  const [recordingError, setRecordingError] = React.useState<string | null>(null);

  const settings = React.useMemo(() => {
    return {
      media: { camera: true, microphone: true, label: 'Media Devices', speaker: true },
      recording: RECORDING_ENABLED ? { label: 'Recording' } : undefined,
    };
  }, []);

  const tabs = React.useMemo(
    () =>
      Object.keys(settings).filter((t) => settings[t as keyof typeof settings]) as Array<
        keyof typeof settings
      >,
    [settings],
  );
  const [activeTab, setActiveTab] = React.useState(tabs[0]);

  React.useEffect(() => {
    if (!isRecording) {
      setEgressId(null);
      setProcessingRecRequest(false);
    }
  }, [isRecording]);

  const toggleRoomRecording = async () => {
    setRecordingError(null);
    if (room.isE2EEEnabled) {
      setRecordingError('Recording of encrypted meetings is currently not supported');
      return;
    }
    setProcessingRecRequest(true);
    try {
      if (isRecording && egressId) {
        const response = await fetch(`/api/rooms/${encodeURIComponent(room.name)}/record/stop`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ egressId }),
        });
        if (!response.ok) {
          const message = await response.text();
          throw new Error(message);
        }
        setEgressId(null);
      } else {
        const response = await fetch(`/api/rooms/${encodeURIComponent(room.name)}/record/start`, {
          method: 'POST',
        });
        if (!response.ok) {
          const message = await response.text();
          throw new Error(message);
        }
        const data = await response.json();
        setEgressId(data.egressId);
      }
    } catch (error) {
      console.error('Error handling recording request:', error);
      setRecordingError(error instanceof Error ? error.message : 'Unknown error');
    } finally {
      setProcessingRecRequest(false);
    }
  };

  return (
    <div className="settings-menu" style={{ width: '100%', position: 'relative' }} {...props}>
      <div className={styles.tabs}>
        {tabs.map(
          (tab) =>
            settings[tab] && (
              <button
                className={`${styles.tab} lk-button`}
                key={tab}
                onClick={() => setActiveTab(tab)}
                aria-pressed={tab === activeTab}
              >
                {
                  // @ts-ignore
                  settings[tab].label
                }
              </button>
            ),
        )}
      </div>
      <div className="tab-content">
        {activeTab === 'media' && (
          <>
            {settings.media && settings.media.camera && (
              <>
                <h3>Camera</h3>
                <section>
                  <CameraSettings />
                </section>
              </>
            )}
            {settings.media && settings.media.microphone && (
              <>
                <h3>Microphone</h3>
                <section>
                  <MicrophoneSettings />
                </section>
              </>
            )}
            {settings.media && settings.media.speaker && (
              <>
                <h3>Speaker & Headphones</h3>
                <section className="lk-button-group">
                  <span className="lk-button">Audio Output</span>
                  <div className="lk-button-group-menu">
                    <MediaDeviceMenu kind="audiooutput"></MediaDeviceMenu>
                  </div>
                </section>
              </>
            )}
          </>
        )}
        {activeTab === 'recording' && (
          <>
            <h3>Record Meeting</h3>
            <section>
              <p>
                {isRecording
                  ? 'Meeting is currently being recorded'
                  : 'No active recordings for this meeting'}
              </p>
              <button
                className="lk-button"
                disabled={processingRecRequest}
                onClick={() => toggleRoomRecording()}
              >
                {isRecording ? 'Stop' : 'Start'} Recording
              </button>
              {recordingError && (
                <p style={{ color: 'var(--lk-danger2)', marginTop: '0.5rem' }}>{recordingError}</p>
              )}
            </section>
          </>
        )}
      </div>
      <div style={{ display: 'flex', justifyContent: 'flex-end', width: '100%' }}>
        <button
          className={`lk-button`}
          onClick={() => layoutContext?.widget.dispatch?.({ msg: 'toggle_settings' })}
        >
          Close
        </button>
      </div>
    </div>
  );
}
