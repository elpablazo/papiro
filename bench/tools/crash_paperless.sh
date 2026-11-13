#!/usr/bin/env bash
# Interrupciones (SIGKILL del contenedor principal) en Paperless sobre el stack «pl» ya levantado.
#   api:     se sube un escaneo por la API y se mata el contenedor durante el OCR
#   carpeta: se deja un escaneo en consume/ y se mata el contenedor durante el OCR
set -uo pipefail
cd "$(dirname "$0")/.."
source .secrets
U=http://127.0.0.1:8010
tok() { curl -s -H 'Content-Type: application/json' -d "{\"username\":\"admin\",\"password\":\"$ADMIN_PASSWORD\"}" $U/api/token/ | jq -r .token; }
H() { echo "Authorization: Token $(tok)"; }
count() { curl -s -H "$(H)" "$U/api/documents/?page_size=1" | jq .count; }
found() { curl -s -H "$(H)" "$U/api/documents/?query=%22$1%22&page_size=1" | jq .count; }
f=$(ls ocr/recto.pdf); ref=OCR-ES-4471
case "$1" in
api)
  before=$(count)
  TASK=$(curl -s -H "$(H)" -F document=@"$f" $U/api/documents/post_document/ | jq -r .)
  sleep 6; echo "estado antes del corte: $(curl -s -H "$(H)" -H 'Accept: application/json; version=10' "$U/api/tasks/?task_id=$TASK" | jq -r '.results[0].status')"
  podman kill -s KILL pl-web >/dev/null; podman start pl-web >/dev/null; tools/paperless.sh wait >/dev/null
  sleep 90
  echo "tarea tras reiniciar: $(curl -s -H "$(H)" -H 'Accept: application/json; version=10' "$U/api/tasks/?task_id=$TASK" | jq -c '.results[0] | {status, result: (.result_data|tostring|.[0:120])}')"
  echo "documentos: $before -> $(count); ¿se encuentra el folio $ref?: $(found $ref)"
  echo "archivos en scratch: $(podman exec pl-web sh -c 'find /tmp/paperless -type f -name "*.pdf" 2>/dev/null | wc -l')" ;;
carpeta)
  before=$(count)
  podman cp "$f" pl-web:/usr/src/paperless/consume/corte-carpeta.pdf
  sleep 14; podman kill -s KILL pl-web >/dev/null; podman start pl-web >/dev/null; tools/paperless.sh wait >/dev/null
  sleep 120
  echo "documentos: $before -> $(count); en consume/: $(podman exec pl-web ls /usr/src/paperless/consume | tr '\n' ' ')" ;;
esac
