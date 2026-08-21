---
title: OIDC Setup
description: Configuring OpenID Connect authentication for LiveKit Meet.
category: guides
order: 6
nav_order: 6
---

# OIDC Setup

LiveKit Meet uses [Auth.js](https://authjs.dev/) with a generic OIDC provider. Any provider exposing a standard discovery document is supported.

## sso-gateway

For Sunbeam deployments, the recommended provider is [sso-gateway](../sso-gateway/README.md). Provision an OIDC client with:

- **Client ID**: `meet` (or any identifier you configure)
- **Redirect URI**: `https://meet.sunbeam.pt/api/auth/callback/oidc`
- **Scopes**: `openid`, `email`, `profile`

Then set the environment variables:

```env
OIDC_ISSUER=https://sso.sunbeam.pt
OIDC_CLIENT_ID=meet
OIDC_CLIENT_SECRET=...
AUTH_URL=https://meet.sunbeam.pt
AUTH_SECRET=...
```

## Other providers

For providers like Keycloak, Authentik, or Google Workspace:

1. Create an OAuth2/OIDC client.
2. Add the callback URL `https://<your-domain>/api/auth/callback/oidc`.
3. Ensure the `email` claim is returned in the ID token or userinfo response.
4. Set `OIDC_ISSUER`, `OIDC_CLIENT_ID`, and `OIDC_CLIENT_SECRET`.

## Test authentication

For local Playwright runs, enable the test provider:

```env
ALLOW_TEST_AUTH=true
```

This exposes a credentials provider that signs in with arbitrary name and email values. It is disabled by default and must never be enabled in production.
