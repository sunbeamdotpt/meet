// SPDX-License-Identifier: AGPL-3.0-or-later
export interface OIDCUserInfo {
  sub: string;
  email?: string;
  name?: string;
}

interface DiscoveryDocument {
  userinfo_endpoint?: string;
}

const DISCOVERY_CACHE_TTL_MS = 10 * 60 * 1000; // 10 minutes
let discoveryCache: DiscoveryDocument | null = null;
let discoveryCacheExpiry = 0;

export function clearDiscoveryCache(): void {
  discoveryCache = null;
  discoveryCacheExpiry = 0;
}

function getIssuer(): string {
  const issuer = process.env.OIDC_ISSUER;
  if (!issuer) {
    throw new Error('OIDC_ISSUER environment variable is not defined');
  }
  return issuer.replace(/\/+$/, '');
}

export async function fetchDiscovery(issuerUrl: string): Promise<DiscoveryDocument> {
  if (discoveryCache && Date.now() < discoveryCacheExpiry) {
    return discoveryCache;
  }

  const url = `${issuerUrl}/.well-known/openid-configuration`;
  const res = await fetch(url);
  if (!res.ok) {
    throw new Error(`OIDC discovery failed: HTTP ${res.status}`);
  }

  const discovery = (await res.json()) as DiscoveryDocument;
  discoveryCache = discovery;
  discoveryCacheExpiry = Date.now() + DISCOVERY_CACHE_TTL_MS;
  return discovery;
}

export async function verifyBearerToken(token: string): Promise<OIDCUserInfo | null> {
  try {
    const issuer = getIssuer();
    const discovery = await fetchDiscovery(issuer);

    if (!discovery.userinfo_endpoint) {
      throw new Error('OIDC discovery document is missing userinfo_endpoint');
    }

    const res = await fetch(discovery.userinfo_endpoint, {
      headers: { Authorization: `Bearer ${token}` },
    });

    if (!res.ok) {
      return null;
    }

    const userinfo = (await res.json()) as OIDCUserInfo;
    if (!userinfo.sub) {
      return null;
    }

    return userinfo;
  } catch (err) {
    if (err instanceof Error) {
      console.error('Bearer token verification failed:', err.message);
    }
    return null;
  }
}
