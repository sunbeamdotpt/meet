import NextAuth from 'next-auth';
import type { OIDCConfig } from 'next-auth/providers';

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

export const { handlers, signIn, signOut, auth } = NextAuth({
  providers: [oidcProvider],
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
