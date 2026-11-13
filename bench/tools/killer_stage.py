#!/usr/bin/env python3
"""SIGKILL cuando aparece la carpeta temporal de guardado (tmp/stage-*), más un retardo: corta a mitad de la copia verificada."""
import os, signal, sys, time
pid, tmp, delay = int(sys.argv[1]), sys.argv[2], float(sys.argv[3])
end = time.time() + 120
while time.time() < end:
    if any(n.startswith("stage-") for n in os.listdir(tmp)):
        time.sleep(delay); os.kill(pid, signal.SIGKILL); print(f"SIGKILL a mitad del guardado (+{delay}s)"); sys.exit(0)
sys.exit(1)
