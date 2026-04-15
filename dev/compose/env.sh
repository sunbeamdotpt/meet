#!/usr/bin/env bash
# Emit the env vars `cargo nextest run --profile integration` needs, in
# direnv-compatible `export` form. The repo's .envrc already exports the same
# values pointing at the local docker-compose stack — this script exists for
# CI or shells without direnv.
#
# Pipe into a per-shell sourceable file:
#   ./env.sh > ../../.envrc.local && (cd ../.. && direnv allow)
set -eu

cat <<'EOF'
export DATABASE_URL=postgres://meet:meet@127.0.0.1:5432/meet
export VALKEY_URL=redis://127.0.0.1:6379
export NATS_URL=nats://127.0.0.1:4222
export S3_ENDPOINT=http://127.0.0.1:8333
export S3_ACCESS_KEY=any
export S3_SECRET_KEY=any
export S3_REGION=us-east-1
export S3_BUCKET=sunbeam-meet-it
export CALDAV_URL=http://admin:admin@127.0.0.1:8090/dav/
export CALDAV_USER=admin
export CALDAV_PASSWORD=admin
export LIVEKIT_URL=http://127.0.0.1:7880
export LIVEKIT_API_KEY=devkey
export LIVEKIT_API_SECRET=devsecretdevsecretdevsecretdevsecretXX
export KETO_READ_URL=http://127.0.0.1:4466
export KETO_WRITE_URL=http://127.0.0.1:4467
export KRATOS_PUBLIC_URL=http://127.0.0.1:4433
export KRATOS_ADMIN_URL=http://127.0.0.1:4434
EOF
