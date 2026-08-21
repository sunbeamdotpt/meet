import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { createParticipantToken } from '@/lib/token';

const instances: Array<{
  apiKey: string | undefined;
  apiSecret: string | undefined;
  userInfo: Record<string, unknown>;
  grant?: Record<string, unknown>;
  ttl: string;
}> = [];

vi.mock('livekit-server-sdk', () => ({
  AccessToken: vi.fn().mockImplementation(function (this: Record<string, unknown>, apiKey, apiSecret, userInfo) {
    const instance = {
      apiKey,
      apiSecret,
      userInfo,
      ttl: '',
      grant: undefined as Record<string, unknown> | undefined,
    };
    instances.push(instance);
    this.addGrant = (grant: Record<string, unknown>) => {
      instance.grant = grant;
    };
    this.toJwt = () => 'fake-jwt';

    Object.defineProperty(this, 'ttl', {
      set(value: string) {
        instance.ttl = value;
      },
      get() {
        return instance.ttl;
      },
    });
  }),
}));

describe('createParticipantToken', () => {
  const OLD_ENV = process.env;

  beforeEach(() => {
    process.env = { ...OLD_ENV, LIVEKIT_API_KEY: 'key', LIVEKIT_API_SECRET: 'secret' };
    instances.length = 0;
  });

  afterEach(() => {
    process.env = OLD_ENV;
  });

  it('grants full host permissions including roomAdmin', async () => {
    await createParticipantToken(
      {
        identity: 'user@example.com__abcd',
        name: 'Host',
        role: 'host',
      },
      'room-1',
    );

    expect(instances).toHaveLength(1);
    expect(instances[0].grant).toEqual({
      room: 'room-1',
      roomJoin: true,
      canPublish: true,
      canPublishData: true,
      canSubscribe: true,
      roomAdmin: true,
    });
  });

  it('grants restricted guest permissions', async () => {
    await createParticipantToken(
      {
        identity: 'user@example.com__abcd',
        name: 'Guest',
        role: 'guest',
      },
      'room-1',
    );

    expect(instances[0].grant).toEqual({
      room: 'room-1',
      roomJoin: true,
      canPublish: false,
      canPublishData: false,
      canSubscribe: false,
      roomAdmin: false,
    });
  });

  it('sets a 5 minute ttl', async () => {
    await createParticipantToken(
      {
        identity: 'user@example.com__abcd',
        name: 'Host',
        role: 'host',
      },
      'room-1',
    );

    expect(instances[0].ttl).toBe('5m');
  });
});
