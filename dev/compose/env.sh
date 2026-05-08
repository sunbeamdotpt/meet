#!/usr/bin/env bash
# Emit env vars for the sunbeam-meet INTEGRATION-TEST stack.
# These use remapped host ports (+10000) so they do NOT conflict with
# workspace services. Run this stack with:
#   (cd dev/compose && docker compose up -d)
#
# For daily dev, use `sunbeam ops compose up sunbeam-meet` instead — the repo's
# .envrc points at workspace ports (5432, 6379, 4222, etc.).
#
# Pipe into a per-shell sourceable file:
#   ./env.sh > ../../.envrc.local && (cd ../.. && direnv allow)
set -eu

cat <<'EOF'
export DATABASE_URL=postgres://meet:meet@127.0.0.1:15432/meet
export VALKEY_URL=redis://127.0.0.1:16379
export NATS_URL=nats://127.0.0.1:14222
export S3_ENDPOINT=http://127.0.0.1:18333
export S3_ACCESS_KEY=any
export S3_SECRET_KEY=any
export S3_REGION=us-east-1
export S3_BUCKET=sunbeam-meet-it
export CALDAV_URL=http://admin:admin@127.0.0.1:18090/dav/
export CALDAV_USER=admin
export CALDAV_PASSWORD=admin
export LIVEKIT_URL=http://127.0.0.1:17880
export LIVEKIT_API_KEY=devkey
export LIVEKIT_API_SECRET=devsecretdevsecretdevsecretdevsecretXX
export KETO_READ_URL=http://127.0.0.1:14466
export KETO_WRITE_URL=http://127.0.0.1:14467
export KRATOS_PUBLIC_URL=http://127.0.0.1:14433
export KRATOS_ADMIN_URL=http://127.0.0.1:14434
EOF
