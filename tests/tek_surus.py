# Tek komutlu manuel sürücü: tüm olayları ve stderr'i basar
# Kullanım: python tests/tek_surus.py <url> <dosyaAdı> [eslik]
import json
import os
import struct
import subprocess
import sys
import threading
import time

KOK = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
EXE = os.path.join(KOK, "core", "target", "release", "titan-downloader-core.exe")

url = sys.argv[1]
ad = sys.argv[2] if len(sys.argv) > 2 else "manuel.bin"
eslik = int(sys.argv[3]) if len(sys.argv) > 3 else 8

islem = subprocess.Popen([EXE], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=None)


def oku():
    while True:
        bas = islem.stdout.read(4)
        if len(bas) < 4:
            print("[stdout kapandı]")
            return
        (n,) = struct.unpack("<I", bas)
        print(json.dumps(json.loads(islem.stdout.read(n)), ensure_ascii=False), flush=True)


threading.Thread(target=oku, daemon=True).start()

veri = json.dumps({
    "action": "START_DOWNLOAD", "url": url, "filename": ad,
    "headers": {}, "concurrency": eslik,
}).encode()
islem.stdin.write(struct.pack("<I", len(veri)))
islem.stdin.write(veri)
islem.stdin.flush()

try:
    son = time.time() + 120
    while time.time() < son:
        if islem.poll() is not None:
            print(f"[çekirdek çıktı: {islem.returncode}]")
            break
        time.sleep(0.5)
finally:
    try:
        islem.kill()
    except Exception:
        pass
