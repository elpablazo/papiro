#!/usr/bin/env bash
# Pruebas de interrupción (SIGKILL) sobre Papiro en un contenedor aparte (puerto 8021, volumen pcrash-*).
# Uso: crash_papiro.sh <escenario>   escenarios: copia | ocr | subida | carpeta | importacion
set -uo pipefail
cd "$(dirname "$0")/.."
source .secrets
N=pcrash; PORT=8021; IMG=papiro:dev; U=http://127.0.0.1:$PORT
fresh() {
  podman rm -f $N >/dev/null 2>&1; podman volume rm -f $N-data $N-consume >/dev/null 2>&1
  podman run -d --name $N -p $PORT:8000 -v $N-data:/data -v $N-consume:/consume -e PAPIRO_CONSUME_INTERVAL=1 \
    -e PAPIRO_ADMIN_USER=admin -e PAPIRO_ADMIN_PASSWORD="$ADMIN_PASSWORD" $IMG >/dev/null
  up; jar=$(mktemp)
  curl -sf -c "$jar" -H 'Content-Type: application/json' -d "{\"username\":\"admin\",\"password\":\"$ADMIN_PASSWORD\"}" $U/api/auth/login >/dev/null
  TOK=$(curl -sf -b "$jar" -H 'X-Requested-With: papiro' -H 'Content-Type: application/json' -d '{"name":"crash"}' $U/api/tokens | jq -r .token); rm -f "$jar"
  H="Authorization: Bearer $TOK"
  curl -sf -H "$H" -F file=@docs/digital_01.pdf "$U/api/documents?wait=true" >/dev/null && echo "base: 1 documento"
}
up() { until curl -sf -o /dev/null $U/api/health; do sleep 0.05; done; }
pid() { podman inspect -f '{{.State.Pid}}' $N; }
mp() { podman volume inspect $1 -f '{{.Mountpoint}}'; }
settle() { podman start $N >/dev/null; up; for _ in $(seq 120); do [ "$(curl -sf -H "$H" $U/api/stats | jq .queued)" = 0 ] && break; sleep 1; done; }
report() {
  echo "--- tras reiniciar ---"
  curl -sf -H "$H" $U/api/jobs | jq -c '.[] | {filename, status, error, doc_id}'
  local id; id=$(curl -sf -H "$H" "$U/api/documents?q=$1&limit=1" | jq '.items[0].id // empty')
  if [ -n "$id" ]; then
    curl -s -o /dev/null -w "documento #$id: descarga /file -> %{http_code} (%{size_download} bytes)\n" -H "$H" "$U/api/documents/$id/file?variant=original"
  else echo "el documento NO aparece en la búsqueda"; fi
  podman exec $N papiro verify 2>&1 | tail -4; podman unshare python3 tools/integrity.py "$(mp $N-data)" | jq -c "{documents, problems, orphan_dirs, tmp_files, jobs}"
}
case "$1" in
copia)   # corte justo después de confirmar la fila en la base y antes/durante la copia del original
  fresh; podman unshare python3 tools/killer.py "$(pid)" "$(mp $N-data)/docs" "${2:-0}" 120 & K=$!
  curl -s -o /dev/null -H "$H" -F file=@scale/grande.pdf $U/api/documents; wait $K; settle; report FOLIO-CORTE-001 ;;
ocr)     # corte en plena extracción/OCR, antes de tocar la base
  fresh; f=$(ls scale/scan100/*.pdf | head -1); ref=$(basename "$f" .pdf)
  podman unshare python3 tools/killer.py "$(pid)" "$(mp $N-data)/tmp" 1.5 120 & K=$!
  curl -s -o /dev/null -H "$H" -F file=@"$f" $U/api/documents; wait $K; settle; report "$ref" ;;
subida)  # corte a mitad de una subida lenta
  fresh; podman unshare python3 tools/killer.py "$(pid)" "$(mp $N-data)/tmp" 2 120 & K=$!
  curl -s -o /dev/null --limit-rate 2M -H "$H" -F file=@scale/grande.pdf $U/api/documents; wait $K; settle; report FOLIO-CORTE-001 ;;
carpeta) # lo mismo que «copia», pero el archivo entra por la carpeta vigilada
  fresh; podman unshare python3 tools/killer.py "$(pid)" "$(mp $N-data)/docs" 0 120 & K=$!
  podman cp scale/grande.pdf $N:/consume/grande.pdf; wait $K; settle
  echo "en /consume tras reiniciar: $(podman exec $N ls -A /consume | tr '\n' ' ')"; report FOLIO-CORTE-001 ;;
importacion) # corte a mitad de `papiro import`, luego se repite la importación
  fresh; podman stop $N >/dev/null
  podman run -d --name $N-imp -v $N-data:/data -v "$PWD/export":/import:ro,z $IMG import /import >/dev/null
  podman unshare python3 tools/killer.py "$(podman inspect -f '{{.State.Pid}}' $N-imp)" "$(mp $N-data)/docs" 0.4 60
  podman rm -f $N-imp >/dev/null
  echo "--- segunda importación ---"; podman run --rm -v $N-data:/data -v "$PWD/export":/import:ro,z $IMG import /import | sed -n 2,8p
  podman start $N >/dev/null; up; report REF-512379 ;;
esac
