'use client';

import React from 'react';
import { decodePassphrase } from '@/lib/client-utils';
import { HostControls } from '@/lib/HostControls';
import { KeyboardShortcuts } from '@/lib/KeyboardShortcuts';
import { RecordingIndicator } from '@/lib/RecordingIndicator';
import { SettingsMenu } from '@/lib/SettingsMenu';
import { ConnectionDetails, MeetingRole } from '@/lib/types';
import {
  formatChatMessageLinks,
  LocalUserChoices,
  PreJoin,
  RoomContext,
  useDataChannel,
  useRoomContext,
  VideoConference,
} from '@livekit/components-react';
import {
  ExternalE2EEKeyProvider,
  RoomOptions,
  VideoCodec,
  VideoPresets,
  Room,
  DeviceUnsupportedError,
  RoomConnectOptions,
  RoomEvent,
  TrackPublishDefaults,
  VideoCaptureOptions,
} from 'livekit-client';
import { useRouter } from 'next/navigation';
import { useSetupE2EE } from '@/lib/useSetupE2EE';
import { useLowCPUOptimizer } from '@/lib/usePerfomanceOptimiser';
import {
  MeetingControls,
  ReactionOverlay,
  RaisedHandsOverlay,
} from '@/lib/MeetingControls';
import { BreakoutControls } from '@/lib/BreakoutControls';

const CONN_DETAILS_ENDPOINT =
  process.env.NEXT_PUBLIC_CONN_DETAILS_ENDPOINT ?? '/api/connection-details';
const SHOW_SETTINGS_MENU = process.env.NEXT_PUBLIC_SHOW_SETTINGS_MENU == 'true';

export function PageClientImpl(props: {
  roomName: string;
  region?: string;
  hq: boolean;
  codec: VideoCodec;
  singlePeerConnection: boolean;
  userName: string;
  role: MeetingRole;
}) {
  const [preJoinChoices, setPreJoinChoices] = React.useState<LocalUserChoices | undefined>(
    undefined,
  );
  const [connectionDetails, setConnectionDetails] = React.useState<ConnectionDetails | undefined>(
    undefined,
  );
  const [waiting, setWaiting] = React.useState(false);
  const [admitted, setAdmitted] = React.useState(false);
  const [waitingCount, setWaitingCount] = React.useState(0);

  const preJoinDefaults = React.useMemo(() => {
    return {
      username: props.userName,
      videoEnabled: true,
      audioEnabled: true,
    };
  }, [props.userName]);

  const fetchConnectionDetails = React.useCallback(
    async (values: LocalUserChoices) => {
      const url = new URL(CONN_DETAILS_ENDPOINT, window.location.origin);
      url.searchParams.append('roomName', props.roomName);
      url.searchParams.append('participantName', values.username);
      url.searchParams.append('role', props.role);
      if (props.region) {
        url.searchParams.append('region', props.region);
      }
      const connectionDetailsResp = await fetch(url.toString());
      if (!connectionDetailsResp.ok) {
        throw new Error(await connectionDetailsResp.text());
      }
      const connectionDetailsData = await connectionDetailsResp.json();
      setConnectionDetails(connectionDetailsData);
    },
    [props.roomName, props.region, props.role],
  );

  const handlePreJoinSubmit = React.useCallback(
    async (values: LocalUserChoices) => {
      setPreJoinChoices(values);
      if (props.role === 'host') {
        await fetchConnectionDetails(values);
        return;
      }

      // Guest: register as waiting and poll for admission.
      setWaiting(true);
      try {
        const knockResp = await fetch(`/api/rooms/${encodeURIComponent(props.roomName)}/knock`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ name: values.username }),
        });
        if (!knockResp.ok) {
          throw new Error(await knockResp.text());
        }
      } catch (error) {
        console.error('Failed to knock:', error);
      }
    },
    [fetchConnectionDetails, props.roomName, props.role],
  );
  const handlePreJoinError = React.useCallback((e: any) => console.error(e), []);

  React.useEffect(() => {
    if (props.role !== 'guest' || !waiting) return;

    let cancelled = false;
    const checkAdmission = async () => {
      try {
        const res = await fetch(`/api/rooms/${encodeURIComponent(props.roomName)}/admission`);
        if (!res.ok) return;
        const data = await res.json();
        if (data.admitted && !cancelled) {
          setAdmitted(true);
          if (preJoinChoices) {
            await fetchConnectionDetails(preJoinChoices);
          }
        }
      } catch (error) {
        console.error('Failed to check admission:', error);
      }
    };

    checkAdmission();
    const interval = window.setInterval(checkAdmission, 1500);
    return () => {
      cancelled = true;
      window.clearInterval(interval);
    };
  }, [props.role, props.roomName, waiting, preJoinChoices, fetchConnectionDetails]);

  React.useEffect(() => {
    if (props.role !== 'guest' || !waiting) return;

    let cancelled = false;
    const fetchWaitingCount = async () => {
      try {
        const res = await fetch(`/api/rooms/${encodeURIComponent(props.roomName)}/waiting`);
        if (!res.ok) return;
        const data = await res.json();
        if (!cancelled) {
          setWaitingCount(data.guests?.length ?? 0);
        }
      } catch (error) {
        // ignore
      }
    };

    fetchWaitingCount();
    const interval = window.setInterval(fetchWaitingCount, 3000);
    return () => {
      cancelled = true;
      window.clearInterval(interval);
    };
  }, [props.role, props.roomName, waiting]);

  if (connectionDetails === undefined || preJoinChoices === undefined) {
    return (
      <main data-lk-theme="default" style={{ height: '100%' }}>
        {waiting ? (
          <WaitingScreen waitingCount={waitingCount} />
        ) : (
          <div style={{ display: 'grid', placeItems: 'center', height: '100%' }}>
            <PreJoin
              defaults={preJoinDefaults}
              onSubmit={handlePreJoinSubmit}
              onError={handlePreJoinError}
            />
          </div>
        )}
      </main>
    );
  }

  return (
    <main data-lk-theme="default" style={{ height: '100%' }}>
      <VideoConferenceComponent
        connectionDetails={connectionDetails}
        userChoices={preJoinChoices}
        options={{
          codec: props.codec,
          hq: props.hq,
          singlePeerConnection: props.singlePeerConnection,
        }}
        role={props.role}
        admitted={admitted}
      />
    </main>
  );
}

function VideoConferenceComponent(props: {
  userChoices: LocalUserChoices;
  connectionDetails: ConnectionDetails;
  options: {
    hq: boolean;
    codec: VideoCodec;
    singlePeerConnection: boolean;
  };
  role: MeetingRole;
  admitted?: boolean;
}) {
  const keyProvider = React.useMemo(() => new ExternalE2EEKeyProvider(), []);
  const { worker, e2eePassphrase } = useSetupE2EE(props.connectionDetails.e2eePassphrase);
  const e2eeEnabled = !!(e2eePassphrase && worker);

  const [e2eeSetupComplete, setE2eeSetupComplete] = React.useState(false);

  const roomOptions = React.useMemo((): RoomOptions => {
    let videoCodec: VideoCodec | undefined = props.options.codec ? props.options.codec : 'vp9';
    if (e2eeEnabled && (videoCodec === 'av1' || videoCodec === 'vp9')) {
      videoCodec = undefined;
    }
    const videoCaptureDefaults: VideoCaptureOptions = {
      deviceId: props.userChoices.videoDeviceId ?? undefined,
      resolution: props.options.hq ? VideoPresets.h2160 : VideoPresets.h720,
    };
    const publishDefaults: TrackPublishDefaults = {
      dtx: false,
      videoSimulcastLayers: props.options.hq
        ? [VideoPresets.h1080, VideoPresets.h720]
        : [VideoPresets.h540, VideoPresets.h216],
      red: !e2eeEnabled,
      videoCodec,
    };
    return {
      videoCaptureDefaults: videoCaptureDefaults,
      publishDefaults: publishDefaults,
      audioCaptureDefaults: {
        deviceId: props.userChoices.audioDeviceId ?? undefined,
      },
      adaptiveStream: true,
      dynacast: true,
      e2ee: keyProvider && worker && e2eeEnabled ? { keyProvider, worker } : undefined,
      singlePeerConnection: props.options.singlePeerConnection,
    };
  }, [
    props.userChoices,
    props.options.hq,
    props.options.codec,
    props.options.singlePeerConnection,
    e2eeEnabled,
    worker,
    keyProvider,
  ]);

  const room = React.useMemo(() => new Room(roomOptions), [roomOptions]);

  React.useEffect(() => {
    if (e2eeEnabled) {
      keyProvider
        .setKey(decodePassphrase(e2eePassphrase))
        .then(() => {
          room.setE2EEEnabled(true).catch((e) => {
            if (e instanceof DeviceUnsupportedError) {
              alert(
                `You're trying to join an encrypted meeting, but your browser does not support it. Please update it to the latest version and try again.`,
              );
              console.error(e);
            } else {
              throw e;
            }
          });
        })
        .then(() => setE2eeSetupComplete(true));
    } else {
      setE2eeSetupComplete(true);
    }
  }, [e2eeEnabled, room, e2eePassphrase, keyProvider]);

  const connectOptions = React.useMemo((): RoomConnectOptions => {
    return {
      autoSubscribe: true,
    };
  }, []);

  const router = useRouter();
  const handleOnLeave = React.useCallback(() => router.push('/'), [router]);
  const handleError = React.useCallback((error: Error) => {
    console.error(error);
    alert(`Encountered an unexpected error, check the console logs for details: ${error.message}`);
  }, []);
  const handleEncryptionError = React.useCallback((error: Error) => {
    console.error(error);
    alert(
      `Encountered an unexpected encryption error, check the console logs for details: ${error.message}`,
    );
  }, []);

  React.useEffect(() => {
    room.on(RoomEvent.Disconnected, handleOnLeave);
    room.on(RoomEvent.EncryptionError, handleEncryptionError);
    room.on(RoomEvent.MediaDevicesError, handleError);

    if (e2eeSetupComplete) {
      room
        .connect(
          props.connectionDetails.serverUrl,
          props.connectionDetails.participantToken,
          connectOptions,
        )
        .catch((error) => {
          handleError(error);
        });
      if (props.userChoices.videoEnabled) {
        room.localParticipant.setCameraEnabled(true).catch((error) => {
          handleError(error);
        });
      }
      if (props.userChoices.audioEnabled) {
        room.localParticipant.setMicrophoneEnabled(true).catch((error) => {
          handleError(error);
        });
      }
    }
    return () => {
      room.off(RoomEvent.Disconnected, handleOnLeave);
      room.off(RoomEvent.EncryptionError, handleEncryptionError);
      room.off(RoomEvent.MediaDevicesError, handleError);
    };
  }, [
    e2eeSetupComplete,
    room,
    props.connectionDetails,
    props.userChoices,
    connectOptions,
    handleOnLeave,
    handleEncryptionError,
    handleError,
  ]);

  const lowPowerMode = useLowCPUOptimizer(room);

  React.useEffect(() => {
    if (lowPowerMode) {
      console.warn('Low power mode enabled');
    }
  }, [lowPowerMode]);

  return (
    <div className="lk-room-container" style={{ position: 'relative', height: '100%' }}>
      <RoomContext.Provider value={room}>
        {props.role === 'host' && <HostControls />}
        {props.role === 'host' && <BreakoutControls />}
        <RoomLayer role={props.role} userName={props.userChoices.username} />
        <KeyboardShortcuts />
        <VideoConference
          chatMessageFormatter={formatChatMessageLinks}
          SettingsComponent={SHOW_SETTINGS_MENU ? SettingsMenu : undefined}
        />
        <RecordingIndicator />
      </RoomContext.Provider>
    </div>
  );
}

interface ReactionItem {
  id: string;
  emoji: string;
  left: number;
  createdAt: number;
}

interface RaisedHandItem {
  identity: string;
  name: string;
  raisedAt: number;
}

interface BreakoutAssignmentNotice {
  type: 'assignment';
  roomName: string;
  label: string;
}

interface BreakoutCloseNotice {
  type: 'close';
}

type BreakoutNotice = BreakoutAssignmentNotice | BreakoutCloseNotice;

function RoomLayer(props: { role: MeetingRole; userName: string }) {
  const room = useRoomContext();
  const [reactions, setReactions] = React.useState<ReactionItem[]>([]);
  const [raisedHands, setRaisedHands] = React.useState<RaisedHandItem[]>([]);
  const [breakoutNotice, setBreakoutNotice] = React.useState<BreakoutNotice | null>(null);

  const addReaction = React.useCallback((emoji: string) => {
    const id = `${Date.now()}-${Math.random().toString(36).slice(2)}`;
    const left = 5 + Math.random() * 90;
    const createdAt = Date.now();
    setReactions((prev) => [...prev, { id, emoji, left, createdAt }]);
    window.setTimeout(() => {
      setReactions((prev) => prev.filter((r) => r.id !== id));
    }, 2000);
  }, []);

  const updateRaisedHand = React.useCallback(
    (raised: boolean, identity: string, name: string) => {
      if (raised) {
        setRaisedHands((prev) => {
          const without = prev.filter((h) => h.identity !== identity);
          return [...without, { identity, name, raisedAt: Date.now() }];
        });
      } else {
        setRaisedHands((prev) => prev.filter((h) => h.identity !== identity));
      }
    },
    [],
  );

  const { message: reactionMessage } = useDataChannel('reaction');
  React.useEffect(() => {
    if (!reactionMessage) return;
    const payload = 'payload' in reactionMessage ? (reactionMessage as any).payload : undefined;
    if (!payload) return;
    try {
      const data = JSON.parse(new TextDecoder().decode(payload)) as { emoji?: string };
      if (data.emoji) addReaction(data.emoji);
    } catch {
      // ignore malformed messages
    }
  }, [reactionMessage, addReaction]);

  const { message: raiseHandMessage } = useDataChannel('raise-hand');
  React.useEffect(() => {
    if (!raiseHandMessage) return;
    const payload = 'payload' in raiseHandMessage ? (raiseHandMessage as any).payload : undefined;
    if (!payload) return;
    try {
      const data = JSON.parse(new TextDecoder().decode(payload)) as {
        identity?: string;
        name?: string;
        raised?: boolean;
      };
      if (data.identity) {
        updateRaisedHand(data.raised ?? true, data.identity, data.name || data.identity);
      }
    } catch {
      // ignore malformed messages
    }
  }, [raiseHandMessage, updateRaisedHand]);

  React.useEffect(() => {
    if (!room.name) return;
    let cancelled = false;
    fetch(`/api/rooms/${encodeURIComponent(room.name)}/breakouts`)
      .then((res) => (res.ok ? res.json() : null))
      .then((data: { state: { active: boolean; assignments?: Record<string, string>; rooms?: Array<{ name: string; label: string }> } | null } | null) => {
        if (cancelled || !data?.state?.active) return;
        const assignment = data.state.assignments?.[room.localParticipant.identity];
        if (assignment) {
          const label = data.state.rooms?.find((r) => r.name === assignment)?.label ?? 'Breakout';
          setBreakoutNotice({ type: 'assignment', roomName: assignment, label });
        }
      })
      .catch((e) => console.error('Failed to fetch breakout state:', e));
    return () => {
      cancelled = true;
    };
  }, [room.name, room.localParticipant.identity]);

  const { message: breakoutMessage } = useDataChannel('breakout');
  React.useEffect(() => {
    if (!breakoutMessage) return;
    const payload = 'payload' in breakoutMessage ? (breakoutMessage as any).payload : undefined;
    if (!payload) return;
    try {
      const data = JSON.parse(new TextDecoder().decode(payload)) as {
        type?: string;
        roomName?: string;
        label?: string;
      };
      if (data.type === 'breakout-assignment' && data.roomName) {
        setBreakoutNotice({ type: 'assignment', roomName: data.roomName, label: data.label ?? 'Breakout' });
      } else if (data.type === 'breakout-close') {
        setBreakoutNotice({ type: 'close' });
      }
    } catch {
      // ignore malformed messages
    }
  }, [breakoutMessage]);

  const joinBreakout = (roomName: string) => {
    window.location.assign(`/rooms/${encodeURIComponent(roomName)}?role=host`);
  };

  const returnToMainRoom = () => {
    window.location.assign(`/rooms/${encodeURIComponent(room.name)}?role=host`);
  };

  return (
    <>
      <MeetingControls
        userName={props.userName}
        onReaction={addReaction}
        onRaiseHand={updateRaisedHand}
      />
      <ReactionOverlay reactions={reactions} />
      <RaisedHandsOverlay raisedHands={raisedHands} />
      {breakoutNotice && (
        <div
          data-testid="breakout-banner"
          style={{
            position: 'absolute',
            top: '4.5rem',
            left: '50%',
            transform: 'translateX(-50%)',
            zIndex: 20,
            backgroundColor: 'var(--lk-bg)',
            border: '1px solid var(--lk-primary)',
            borderRadius: '0.5rem',
            padding: '1rem',
            minWidth: '260px',
            textAlign: 'center',
            boxShadow: '0 4px 12px rgba(0,0,0,0.3)',
          }}
        >
          {breakoutNotice.type === 'assignment' ? (
            <>
              <p style={{ margin: '0 0 0.75rem 0' }}>
                You&apos;ve been assigned to <strong>{breakoutNotice.label}</strong>.
              </p>
              <button className="lk-button" onClick={() => joinBreakout(breakoutNotice.roomName)}>
                Join breakout room
              </button>
            </>
          ) : (
            <>
              <p style={{ margin: '0 0 0.75rem 0' }}>Breakout rooms have been closed.</p>
              <button className="lk-button" onClick={returnToMainRoom}>
                Return to main room
              </button>
            </>
          )}
        </div>
      )}
    </>
  );
}

function WaitingScreen({ waitingCount }: { waitingCount: number }) {
  return (
    <div
      data-testid="waiting-screen"
      data-lk-theme="default"
      style={{
        display: 'grid',
        placeItems: 'center',
        height: '100%',
        textAlign: 'center',
      }}
    >
      <div
        style={{
          maxWidth: '420px',
          padding: '2rem',
          borderRadius: '1rem',
          backgroundColor: 'var(--lk-bg2)',
        }}
      >
        <h2 style={{ marginTop: 0 }}>Waiting for host</h2>
        <p>You&apos;ll join the meeting once the host admits you.</p>
        {waitingCount > 0 && (
          <p style={{ color: 'var(--lk-text-color-secondary)' }}>
            {waitingCount} {waitingCount === 1 ? 'person' : 'people'} waiting ahead of you.
          </p>
        )}
        <div
          style={{
            marginTop: '1.5rem',
            width: '40px',
            height: '40px',
            border: '3px solid var(--lk-border-color)',
            borderTopColor: 'var(--lk-primary)',
            borderRadius: '50%',
            animation: 'spin 1s linear infinite',
            marginInline: 'auto',
          }}
        />
      </div>
    </div>
  );
}
