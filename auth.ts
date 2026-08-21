import NextAuth from 'next-auth';
import type { OIDCConfig } from 'next-auth/providers';
import Credentials from 'next-auth/providers/credentials';

interface OIDCProfile {
  sub: string;
  name?: string;
  email?: string;
  picture?: string;
}

const oidcProvider: OIDCConfig<OIDCProfile> = {
  id: 'oidc',
  name: 'OIDC',
  type: 'oidc',
  issuer: process.env.OIDC_ISSUER,
  clientId: process.env.OIDC_CLIENT_ID,
  clientSecret: process.env.OIDC_CLIENT_SECRET,
  authorization: {
    params: {
      scope: 'openid email profile',
    },
  },
  profile(profile) {
    return {
      id: profile.sub,
      name: profile.name,
      email: profile.email,
      image: profile.picture,
    };
  },
};

const testProvider =
  process.env.ALLOW_TEST_AUTH === 'true'
    ? Credentials({
        id: 'test',
        name: 'Test Account',
        credentials: {
          name: { label: 'Name', type: 'text' },
          email: { label: 'Email', type: 'email' },
        },
        authorize(credentials) {
          const name = typeof credentials?.name === 'string' ? credentials.name : 'Test User';
          const email = typeof credentials?.email === 'string' ? credentials.email : 'test@example.com';
          return { id: email, name, email };
        },
      })
    : null;

const providers: Array<ReturnType<typeof Credentials> | typeof oidcProvider> = [];
if (process.env.OIDC_ISSUER) {
  providers.push(oidcProvider);
}
if (testProvider) {
  providers.push(testProvider);
}

export const { handlers, signIn, signOut, auth } = NextAuth({
  providers: providers.length > 0 ? providers : ([{ id: 'none', name: 'None', type: 'credentials', authorize: () => null }] as any),
  callbacks: {
    async session({ session, token }) {
      if (token.sub) {
        session.user.id = token.sub;
      }
      if (token.email) {
        session.user.email = token.email;
      }
      if (token.name) {
        session.user.name = token.name;
      }
      return session;
    },
  },
});
