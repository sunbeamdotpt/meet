import { auth } from '@/auth';

export default auth((req) => {
  const pathname = req.nextUrl.pathname;
  const requiresAuth = pathname.startsWith('/rooms') || pathname.startsWith('/meetings');

  if (!req.auth && requiresAuth) {
    const newUrl = new URL('/api/auth/signin', req.nextUrl.origin);
    newUrl.searchParams.set('callbackUrl', pathname + req.nextUrl.search);
    return Response.redirect(newUrl);
  }
});

export const config = {
  matcher: ['/rooms/:path*', '/meetings/:path*'],
};
