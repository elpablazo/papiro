#!/usr/bin/env bash
# Stack oficial de Paperless-ngx (equivalente a docker-compose.postgres-tika.yml) con podman.
# Uso: paperless.sh up | down | wait
set -euo pipefail
cd "$(dirname "$0")/.."
POD=${POD:-pl}; PORT=${PORT:-8010}
[ -f .secrets ] || { umask 077; echo "ADMIN_PASSWORD=$(head -c 18 /dev/urandom | base64 | tr -d '/+=')" > .secrets; echo "SECRET_KEY=$(head -c 32 /dev/urandom | base64 | tr -d '/+=')" >> .secrets; }
source .secrets

wait_http() { # espera a que la UI/API responda; imprime segundos transcurridos
  local t0=$(date +%s.%N)
  until [ "$(curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:$PORT/api/documents/ 2>/dev/null)" = 401 ] || \
        [ "$(curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:$PORT/api/documents/ 2>/dev/null)" = 200 ]; do sleep 0.2; done
  echo "$(date +%s.%N) - $t0" | bc
}

case "${1:-up}" in
up)
  podman pod create --name $POD -p $PORT:8000 >/dev/null
  podman run -d --pod $POD --name $POD-broker -v $POD-redis:/data docker.io/valkey/valkey:9-alpine >/dev/null
  podman run -d --pod $POD --name $POD-db -v $POD-pg:/var/lib/postgresql \
    -e POSTGRES_DB=paperless -e POSTGRES_USER=paperless -e POSTGRES_PASSWORD=paperless docker.io/library/postgres:18 >/dev/null
  podman run -d --pod $POD --name $POD-gotenberg docker.io/gotenberg/gotenberg:8.38 \
    gotenberg --chromium-disable-javascript=true '--chromium-allow-list=^file:///tmp/.*' >/dev/null
  podman run -d --pod $POD --name $POD-tika docker.io/apache/tika:3.3.1.0 >/dev/null
  podman run -d --pod $POD --name $POD-web \
    -v $POD-data:/usr/src/paperless/data -v $POD-media:/usr/src/paperless/media \
    -v $POD-consume:/usr/src/paperless/consume -v $POD-export:/usr/src/paperless/export \
    -e PAPERLESS_REDIS=redis://localhost:6379 -e PAPERLESS_DBHOST=localhost -e PAPERLESS_DBENGINE=postgresql \
    -e PAPERLESS_TIKA_ENABLED=1 -e PAPERLESS_TIKA_GOTENBERG_ENDPOINT=http://localhost:3000 -e PAPERLESS_TIKA_ENDPOINT=http://localhost:9998 \
    -e PAPERLESS_SECRET_KEY="$SECRET_KEY" -e PAPERLESS_OCR_LANGUAGE=spa+eng -e PAPERLESS_TIME_ZONE=America/Mexico_City \
    -e PAPERLESS_ADMIN_USER=admin -e PAPERLESS_ADMIN_PASSWORD="$ADMIN_PASSWORD" ${PAPERLESS_EXTRA_ENV:-} \
    ghcr.io/paperless-ngx/paperless-ngx:latest >/dev/null
  echo "arranque en frío (primer boot, con migraciones): $(wait_http) s"
  ;;
wait) wait_http ;;
down) podman pod rm -f $POD >/dev/null; podman volume rm -f $POD-redis $POD-pg $POD-data $POD-media $POD-consume $POD-export >/dev/null; echo abajo ;;
esac
