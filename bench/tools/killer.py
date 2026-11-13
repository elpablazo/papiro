#!/usr/bin/env python3
"""Mata con SIGKILL un proceso en el instante en que aparece algo nuevo en una carpeta.
Uso (dentro de `podman unshare`): killer.py <pid> <carpeta> [retardo_s] [segundos_max]
Simula un corte de luz o un OOM-kill en un punto preciso del procesamiento."""
import os, signal, sys, time

pid, watch = int(sys.argv[1]), sys.argv[2]
delay = float(sys.argv[3]) if len(sys.argv) > 3 else 0.0
deadline = time.time() + (float(sys.argv[4]) if len(sys.argv) > 4 else 300)
before = set(os.listdir(watch))
while time.time() < deadline:
    new = set(os.listdir(watch)) - before
    if new:
        if delay:
            time.sleep(delay)
        os.kill(pid, signal.SIGKILL)
        print(f"SIGKILL a {pid}: apareció {sorted(new)} (+{delay}s)")
        sys.exit(0)
print("tiempo agotado sin cambios"); sys.exit(1)
