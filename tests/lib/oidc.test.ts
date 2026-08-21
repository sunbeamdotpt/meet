import { describe, it, expect, vi, beforeEach } from 'vitest';
import { verifyBearerToken, fetchDiscovery, clearDiscoveryCache } from '@/lib/oidc';

describe('OIDC Bearer token verification', () => {
  const originalEnv = process.env;

  beforeEach(() => {
    process.env = { ...originalEnv, OIDC_ISSUER: 'https://oidc.example.com' };
    clearDiscoveryCache();
    vi.restoreAllMocks();
  });

  it('returns userinfo when the token is valid', async () => {
    vi.stubGlobal('fetch', vi.fn());

    const discovery = { userinfo_endpoint: 'https://oidc.example.com/userinfo' };
    const userinfo = { sub: 'user-123', email: 'test@example.com', name: 'Test User' };

    vi.mocked(fetch)
      .mockResolvedValueOnce({
        ok: true,
        json: async () => discovery,
      } as Response)
      .mockResolvedValueOnce({
        ok: true,
        json: async () => userinfo,
      } as Response);

    const result = await verifyBearerToken('valid-token');
    expect(result).toEqual(userinfo);
  });

  it('returns null when userinfo endpoint rejects the token', async () => {
    vi.stubGlobal('fetch', vi.fn());

    const discovery = { userinfo_endpoint: 'https://oidc.example.com/userinfo' };

    vi.mocked(fetch)
      .mockResolvedValueOnce({
        ok: true,
        json: async () => discovery,
      } as Response)
      .mockResolvedValueOnce({
        ok: false,
        status: 401,
      } as Response);

    const result = await verifyBearerToken('invalid-token');
    expect(result).toBeNull();
  });

  it('returns null when userinfo response lacks sub', async () => {
    vi.stubGlobal('fetch', vi.fn());

    const discovery = { userinfo_endpoint: 'https://oidc.example.com/userinfo' };

    vi.mocked(fetch)
      .mockResolvedValueOnce({
        ok: true,
        json: async () => discovery,
      } as Response)
      .mockResolvedValueOnce({
        ok: true,
        json: async () => ({ name: 'No Sub' }),
      } as Response);

    const result = await verifyBearerToken('valid-token');
    expect(result).toBeNull();
  });

  it('caches the discovery document', async () => {
    vi.stubGlobal('fetch', vi.fn());

    const discovery = { userinfo_endpoint: 'https://oidc.example.com/userinfo' };
    const userinfo = { sub: 'user-123', email: 'test@example.com' };

    vi.mocked(fetch)
      .mockResolvedValueOnce({
        ok: true,
        json: async () => discovery,
      } as Response)
      .mockResolvedValueOnce({
        ok: true,
        json: async () => userinfo,
      } as Response)
      .mockResolvedValueOnce({
        ok: true,
        json: async () => userinfo,
      } as Response);

    await verifyBearerToken('token-1');
    await verifyBearerToken('token-2');

    expect(fetch).toHaveBeenCalledTimes(3); // discovery once + two userinfo calls
  });
});

describe('OIDC discovery', () => {
  const originalEnv = process.env;

  beforeEach(() => {
    process.env = { ...originalEnv, OIDC_ISSUER: 'https://oidc.example.com/' };
    clearDiscoveryCache();
    vi.restoreAllMocks();
  });

  it('fetches discovery from the well-known endpoint', async () => {
    vi.stubGlobal('fetch', vi.fn());

    const discovery = { userinfo_endpoint: 'https://oidc.example.com/userinfo' };
    vi.mocked(fetch).mockResolvedValueOnce({
      ok: true,
      json: async () => discovery,
    } as Response);

    const result = await fetchDiscovery('https://oidc.example.com');
    expect(result).toEqual(discovery);
    expect(fetch).toHaveBeenCalledWith('https://oidc.example.com/.well-known/openid-configuration');
  });
});
