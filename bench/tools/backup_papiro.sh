#!/usr/bin/env bash
# Respaldo y restauración de Papiro (no existe comando propio): se prueban tres métodos sobre una instancia con datos.
#   1) en frío: detener, empaquetar el volumen, restaurar en un volumen nuevo y arrancar
#   2) en caliente ingenuo: copiar la carpeta de datos mientras hay escrituras
#   3) en caliente con la API de respaldo de SQLite (sqlite3 .backup) + copia de docs/
set -uo pipefail
cd "$(dirname "$0")/.."
source .secrets
N=pbak; PORT=8022; IMG=papiro:dev; U=http://127.0.0.1:$PORT; B=$(mktemp -d -p "$PWD/results" backup.XXXX)
mp() { podman volume inspect $1 -f '{{.Mountpoint}}'; }
up() { for _ in $(seq 200); do curl -sf -o /dev/null $U/api/health && return; sleep 0.05; done; echo "NO ARRANCA"; }
login() { jar=$(mktemp); curl -sf -c "$jar" -H 'Content-Type: application/json' -d "{\"username\":\"$1\",\"password\":\"$2\"}" $U/api/auth/login >/dev/null && echo ok || echo FALLA; rm -f "$jar"; }
summary() { curl -sf -H "Authorization: Bearer $TOK" $U/api/stats | jq -c '{documents, trash}'; curl -sf -H "Authorization: Bearer $TOK" $U/api/webhooks | jq -c '[.[].url]';
  curl -sf -H "Authorization: Bearer $TOK" $U/api/users | jq -c '[.[].username]'; }

# Instancia de partida: export real importado + usuario extra + webhook + token
podman rm -f $N >/dev/null 2>&1; for v in $N-data $N-r1 $N-r2 $N-r3; do podman volume rm -f $v >/dev/null; done
podman run --rm -v $N-data:/data -v "$PWD/export":/import:ro,z $IMG import /import | sed -n 3p
podman run -d --name $N -p $PORT:8000 -v $N-data:/data -e PAPIRO_ADMIN_USER=admin -e PAPIRO_ADMIN_PASSWORD="$ADMIN_PASSWORD" $IMG >/dev/null; up
jar=$(mktemp); curl -sf -c "$jar" -H 'Content-Type: application/json' -d "{\"username\":\"admin\",\"password\":\"$ADMIN_PASSWORD\"}" $U/api/auth/login >/dev/null
TOK=$(curl -sf -b "$jar" -H 'X-Requested-With: papiro' -H 'Content-Type: application/json' -d '{"name":"respaldo"}' $U/api/tokens | jq -r .token); rm -f "$jar"
curl -sf -H "Authorization: Bearer $TOK" -H 'Content-Type: application/json' -d '{"username":"ana","password":"contraseña-de-ana"}' $U/api/users >/dev/null
curl -sf -H "Authorization: Bearer $TOK" -H 'Content-Type: application/json' -d '{"url":"http://127.0.0.1:9/hook","secret":"x"}' $U/api/webhooks >/dev/null
echo "== original =="; summary
SHA0=$(podman unshare sh -c "cd '$(mp $N-data)/docs' && find . -type f -name 'original*' -exec sha256sum {} + | sort | sha256sum" | cut -c1-16)

echo; echo "== 1) en frío =="
podman stop -t 10 $N >/dev/null
podman unshare tar -C "$(mp $N-data)" -cf "$B/frio.tar" . && ls -la "$B/frio.tar" | awk '{printf "respaldo: %.1f MB\n", $5/1048576}'
podman volume create $N-r1 >/dev/null; podman unshare tar -C "$(mp $N-r1)" -xf "$B/frio.tar"
podman rm -f $N >/dev/null; podman run -d --name $N -p $PORT:8000 -v $N-r1:/data $IMG >/dev/null; up
echo "restaurado:"; summary; echo "login admin: $(login admin "$ADMIN_PASSWORD")  login ana: $(login ana contraseña-de-ana)  token: $(curl -s -o /dev/null -w %{http_code} -H "Authorization: Bearer $TOK" $U/api/stats)"
SHA1=$(podman unshare sh -c "cd '$(mp $N-r1)/docs' && find . -type f -name 'original*' -exec sha256sum {} + | sort | sha256sum" | cut -c1-16)
echo "huella de todos los originales: antes $SHA0, después $SHA1"
podman unshare python3 tools/integrity.py "$(mp $N-r1)" | jq -c '{integrity_check, documents, problemas: (.problems | map(.problem) | group_by(.) | map({(.[0]): length}) | add)}'

echo; echo "== 2) en caliente, copiando la carpeta mientras se escribe =="
( for i in $(seq 1 4000); do curl -s -o /dev/null -H "Authorization: Bearer $TOK" -H 'Content-Type: application/json' -X PATCH -d "{\"title\":\"edición $i\"}" $U/api/documents/$(( i % 57 + 1 )); done ) &
W=$!; sleep 2
for k in 1 2 3 4 5; do
  podman volume rm -f $N-r2 >/dev/null; podman volume create $N-r2 >/dev/null
  podman unshare sh -c "cp -a '$(mp $N-r1)/.' '$(mp $N-r2)/'"
  r=$(podman unshare sqlite3 "$(mp $N-r2)/papiro.db" 'PRAGMA integrity_check;' 2>&1 | head -1)
  w=$(podman unshare sh -c "ls '$(mp $N-r2)'" | tr '\n' ' ')
  echo "copia $k: integrity_check=$r | archivos: $w"
done
kill $W 2>/dev/null; wait $W 2>/dev/null

echo; echo "== 3) en caliente con sqlite3 .backup + copia de docs/ =="
( for i in $(seq 1 4000); do curl -s -o /dev/null -H "Authorization: Bearer $TOK" -H 'Content-Type: application/json' -X PATCH -d "{\"title\":\"edición $i\"}" $U/api/documents/$(( i % 57 + 1 )); done ) &
W=$!; sleep 2
podman volume create $N-r3 >/dev/null
podman unshare sh -c "sqlite3 '$(mp $N-r1)/papiro.db' \".backup '$(mp $N-r3)/papiro.db'\" && cp -a '$(mp $N-r1)/docs' '$(mp $N-r3)/' && mkdir -p '$(mp $N-r3)/tmp' && chown -R --reference='$(mp $N-r1)' '$(mp $N-r3)'"
kill $W 2>/dev/null; wait $W 2>/dev/null
podman rm -f $N >/dev/null; podman run -d --name $N -p $PORT:8000 -v $N-r3:/data $IMG >/dev/null; up
echo "restaurado:"; summary; echo "login admin: $(login admin "$ADMIN_PASSWORD")"
podman unshare python3 tools/integrity.py "$(mp $N-r3)" | jq -c '{integrity_check, documents, problemas: (.problems | map(.problem) | group_by(.) | map({(.[0]): length}) | add)}'
podman rm -f $N >/dev/null
