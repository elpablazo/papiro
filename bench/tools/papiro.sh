#!/usr/bin/env bash
# Papiro en un solo contenedor. Uso: papiro.sh up | down    (PAPIRO_EXTRA_ENV="-e CLAVE=valor ..." opcional)
set -euo pipefail
cd "$(dirname "$0")/.."
source .secrets
PORT=8020; IMAGE=${PAPIRO_IMAGE:-papiro:dev}
case "${1:-up}" in
up)
  t0=$(date +%s.%N)
  podman run -d --name papiro -p $PORT:8000 -v papiro-data:/data -v papiro-consume:/consume \
    -e PAPIRO_ADMIN_USER=admin -e PAPIRO_ADMIN_PASSWORD="$ADMIN_PASSWORD" -e PAPIRO_OCR_LANGS=spa+eng ${PAPIRO_EXTRA_ENV:-} $IMAGE >/dev/null
  until curl -sf -o /dev/null http://127.0.0.1:$PORT/api/health; do sleep 0.01; done
  echo "arranque en frío (primer boot, crea la base): $(echo "$(date +%s.%N) - $t0" | bc) s"
  # Token de API para los benchmarks (se guarda en .secrets, no se imprime)
  jar=$(mktemp)
  curl -sf -c "$jar" -H 'Content-Type: application/json' -d "{\"username\":\"admin\",\"password\":\"$ADMIN_PASSWORD\"}" http://127.0.0.1:$PORT/api/auth/login >/dev/null
  tok=$(curl -sf -b "$jar" -H 'X-Requested-With: papiro' -H 'Content-Type: application/json' -H "Origin: http://127.0.0.1:$PORT" -d '{"name":"bench"}' http://127.0.0.1:$PORT/api/tokens | jq -r .token)
  rm -f "$jar"; sed -i '/^PAPIRO_TOKEN=/d' .secrets; echo "PAPIRO_TOKEN=$tok" >> .secrets
  ;;
down) podman rm -f papiro >/dev/null; podman volume rm -f papiro-data papiro-consume >/dev/null; echo abajo ;;
esac
