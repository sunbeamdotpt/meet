# SPDX-License-Identifier: AGPL-3.0-or-later
FROM node:22-slim AS base
RUN npm install -g pnpm@10.18.2

FROM base AS deps
WORKDIR /app
COPY package.json pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile

FROM base AS builder
WORKDIR /app
ARG NEXT_PUBLIC_SHOW_SETTINGS_MENU
ENV NEXT_PUBLIC_SHOW_SETTINGS_MENU=${NEXT_PUBLIC_SHOW_SETTINGS_MENU}
COPY --from=deps /app/node_modules ./node_modules
COPY . .
RUN pnpm build

FROM base AS runner
WORKDIR /app
ENV NODE_ENV=production
RUN groupadd --system --gid 999 nodejs && \
    useradd --system --uid 999 --gid nodejs nextjs
COPY --from=builder /app/public ./public
COPY --from=builder --chown=nextjs:nodejs /app/.next/standalone ./
COPY --from=builder --chown=nextjs:nodejs /app/.next/static ./.next/static
COPY --from=builder --chown=nextjs:nodejs /app/migrations ./migrations
USER nextjs
EXPOSE 3000
ENV PORT=3000
ENV HOSTNAME="0.0.0.0"
CMD ["node", "server.js"]
