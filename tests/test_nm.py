# Titanium çekirdeği Native Messaging protokolü üzerinden uçtan uca sınar.
# Kanıtlar: PING/PONG, 16MB Range indirmesi + SHA-256 eşleşmesi + dergi temizliği,
# HLS (düz ve AES-128) ffprobe doğrulaması, DASH video+ses birleştirme,
# iptal, süreç çökmesiyle WAL devam ettirme, LIST.
import hashlib
import json
import os
import struct
import subprocess
import sys
import threading
import time

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")

KOK = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
EXE = os.path.join(KOK, "core", "target", "release", "titan-downloader-core.exe")
FIXTURE = os.path.join(KOK, "tests", "fixture")
INDIRME_DIZINI = os.path.join(os.environ.get("USERPROFILE", ""), "Downloads")
PORT = 8471

sonuclar = []


def kayit(ad, gecti, not_=""):
    sonuclar.append((ad, gecti, not_))
    print(f"[{'GEÇTİ' if gecti else 'KALDI'}] {ad}" + (f" — {not_}" if not_ else ""))


def cerceve_yaz(stdin, nesne):
    veri = json.dumps(nesne).encode("utf-8")
    stdin.write(struct.pack("<I", len(veri)))
    stdin.write(veri)
    stdin.flush()


class Cekirdek:
    def __init__(self):
        self.islem = subprocess.Popen(
            [EXE], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
        )
        self.olaylar = []
        self.kilit = threading.Condition()
        self.durdur = False
        self.okuyucu = threading.Thread(target=self._oku, daemon=True)
        self.okuyucu.start()

    def _oku(self):
        akis = self.islem.stdout
        while True:
            bas = akis.read(4)
            if len(bas) < 4:
                break
            (uzunluk,) = struct.unpack("<I", bas)
            govde = akis.read(uzunluk)
            if len(govde) < uzunluk:
                break
            with self.kilit:
                self.olaylar.append(json.loads(govde))
                self.kilit.notify_all()

    def gonder(self, nesne):
        cerceve_yaz(self.islem.stdin, nesne)

    def bekle(self, durum, kimlik=None, zaman_asimi=90):
        son = time.time() + zaman_asimi
        with self.kilit:
            while True:
                for olay in self.olaylar:
                    if olay.get("status") == durum and (kimlik is None or olay.get("download_id") == kimlik):
                        return olay
                kalan = son - time.time()
                if kalan <= 0:
                    return None
                self.kilit.wait(min(kalan, 1.0))

    def oldur(self):
        self.islem.kill()
        self.islem.wait(timeout=10)

    def isaret(self):
        with self.kilit:
            return len(self.olaylar)

    def bekle_sonrasi(self, isaret, durum, zaman_asimi=90):
        son = time.time() + zaman_asimi
        with self.kilit:
            while True:
                for olay in self.olaylar[isaret:]:
                    if olay.get("status") == durum:
                        return olay
                kalan = son - time.time()
                if kalan <= 0:
                    return None
                self.kilit.wait(min(kalan, 1.0))

    def baslat_ve_izle(self, komut):
        isaret = self.isaret()
        self.gonder(komut)
        baslangic = self.bekle_sonrasi(isaret, "STARTED", zaman_asimi=30)
        if not baslangic:
            hata = self.bekle_sonrasi(isaret, "FAILED", zaman_asimi=5)
            return None, hata
        kimlik = baslangic.get("download_id")
        son = time.time() + 150
        with self.kilit:
            while True:
                for olay in self.olaylar[isaret:]:
                    if olay.get("status") in ("COMPLETED", "FAILED", "CANCELED") and olay.get("download_id") == kimlik:
                        return kimlik, olay
                kalan = son - time.time()
                if kalan <= 0:
                    return kimlik, None
                self.kilit.wait(min(kalan, 1.0))

    def kapat(self):
        try:
            self.gonder({"action": "SHUTDOWN"})
            self.islem.wait(timeout=10)
        except Exception:
            self.oldur()


def sha256(yol):
    h = hashlib.sha256()
    with open(yol, "rb") as f:
        for parca in iter(lambda: f.read(1 << 20), b""):
            h.update(parca)
    return h.hexdigest()


def ffprobe_akislari(yol):
    r = subprocess.run(
        ["ffprobe", "-v", "error", "-show_entries", "stream=codec_type", "-of", "csv=p=0", yol],
        capture_output=True, text=True,
    )
    if r.returncode != 0:
        return None
    return {satir.strip() for satir in r.stdout.splitlines() if satir.strip()}


def dergi_yolu(yol):
    return yol + ".titan_state"


def temizle(ad):
    adaylar = [os.path.join(INDIRME_DIZINI, ad), os.path.join(INDIRME_DIZINI, ad + " (1)")]
    for a in adaylar:
        for y in (a, dergi_yolu(a)):
            if os.path.exists(y):
                os.remove(y)


def test_ping(cekirdek):
    cekirdek.gonder({"action": "PING"})
    olay = cekirdek.bekle("PONG", zaman_asimi=10)
    kayit("PING/PONG", olay is not None)


def test_range_16mb(cekirdek):
    ad = "titan_test_16mb.bin"
    temizle(ad)
    kaynak = os.path.join(FIXTURE, "test_16mb.bin")
    beklenen = sha256(kaynak)
    olay_komut = {
        "action": "START_DOWNLOAD",
        "url": f"http://127.0.0.1:{PORT}/test_16mb.bin",
        "filename": ad,
        "headers": {},
        "concurrency": 8,
    }
    _, olay = cekirdek.baslat_ve_izle(olay_komut)
    if not olay or olay.get("status") != "COMPLETED":
        kayit("16MB Range + SHA-256", False, olay and olay.get("error") or "sonuç gelmedi")
        return
    yol = olay["final_path"]
    gecti = sha256(yol) == beklenen and not os.path.exists(dergi_yolu(yol))
    kayit("16MB Range + SHA-256", gecti, yol)
    temizle(ad)


def test_hls(cekirdek, alt, ad):
    temizle(ad + ".mp4"); temizle(ad + ".ts")
    olay_komut = {
        "action": "START_DOWNLOAD",
        "url": f"http://127.0.0.1:{PORT}/{alt}/playlist.m3u8",
        "filename": ad,
        "headers": {},
        "concurrency": 8,
    }
    _, olay = cekirdek.baslat_ve_izle(olay_komut)
    if not olay or olay.get("status") != "COMPLETED":
        kayit(f"HLS {alt}", False, olay and olay.get("error") or "sonuç gelmedi")
        return
    yol = olay["final_path"]
    akislar = ffprobe_akislari(yol)
    gecti = akislar is not None and "video" in akislar and "audio" in akislar
    kayit(f"HLS {alt}", gecti, f"{yol} akışlar={akislar}")
    temizle(ad + ".mp4"); temizle(ad + ".ts")


def test_dash(cekirdek):
    ad = "titan_test_dash"
    temizle(ad + ".mp4")
    temizle(ad + ".video.mp4"); temizle(ad + ".audio.m4a")
    olay_komut = {
        "action": "START_DOWNLOAD",
        "url": f"http://127.0.0.1:{PORT}/dash/sunum.mpd",
        "filename": ad,
        "headers": {},
        "concurrency": 8,
    }
    _, olay = cekirdek.baslat_ve_izle(olay_komut)
    if not olay or olay.get("status") != "COMPLETED":
        kayit("DASH birleştirme", False, olay and olay.get("error") or "sonuç gelmedi")
        return
    yol = olay["final_path"]
    akislar = ffprobe_akislari(yol)
    gecti = akislar is not None and "video" in akislar and "audio" in akislar
    kayit("DASH birleştirme", gecti, f"{yol} akışlar={akislar}")
    temizle(ad + ".mp4"); temizle(ad + ".video.mp4"); temizle(ad + ".audio.m4a")


def test_iptal(cekirdek):
    ad = "titan_iptal.bin"
    temizle(ad)
    cekirdek.gonder({
        "action": "START_DOWNLOAD",
        "url": f"http://127.0.0.1:{PORT}/yavas_16mb.bin",
        "filename": ad,
        "headers": {},
        "concurrency": 4,
    })
    cekirdek.bekle("PROGRESS", zaman_asimi=30)
    time.sleep(1.0)
    # download_id STARTED/PROGRESS olaylarından alınır
    kimlik = None
    with cekirdek.kilit:
        for olay in reversed(cekirdek.olaylar):
            if olay.get("status") in ("STARTED", "PROGRESS"):
                kimlik = olay.get("download_id")
                break
    if not kimlik:
        kayit("İptal akışı", False, "download_id bulunamadı")
        return
    cekirdek.gonder({"action": "CANCEL", "download_id": kimlik})
    olay = cekirdek.bekle("CANCELED", kimlik, zaman_asimi=30)
    kayit("İptal akışı", olay is not None)
    temizle(ad)


def test_wal_devam():
    ad = "titan_devam.bin"
    temizle(ad)
    c1 = Cekirdek()
    c1.gonder({
        "action": "START_DOWNLOAD",
        "url": f"http://127.0.0.1:{PORT}/yavas_16mb.bin",
        "filename": ad,
        "headers": {},
        "concurrency": 1,
    })
    # Dergi oluşana kadar bekle (en az bir segment tamamlanmalı), sonra süreci zorla öldür
    son = time.time() + 60
    dergi = None
    while time.time() < son:
        for aday in (ad, ad + " (1)"):
            yol = dergi_yolu(os.path.join(INDIRME_DIZINI, aday))
            if os.path.exists(yol) and os.path.getsize(yol) > 20:
                dergi = yol
                break
        if dergi:
            break
        time.sleep(0.5)
    c1.oldur()
    if not dergi:
        kayit("WAL devam ettirme", False, "dergi oluşmadı")
        temizle(ad)
        return

    c2 = Cekirdek()
    c2.gonder({
        "action": "START_DOWNLOAD",
        "url": f"http://127.0.0.1:{PORT}/yavas_16mb.bin",
        "filename": ad,
        "headers": {},
        "concurrency": 4,
    })
    ilk_progress = c2.bekle("PROGRESS", zaman_asimi=30)
    olay = c2.bekle("COMPLETED", zaman_asimi=180)
    if not olay:
        kayit("WAL devam ettirme", False, "COMPLETED gelmedi")
        c2.kapat(); temizle(ad)
        return
    beklenen = sha256(os.path.join(FIXTURE, "yavas_16mb.bin"))
    yol = olay["final_path"]
    devam_var = ilk_progress and ilk_progress.get("downloaded_bytes", 0) > 0
    gecti = sha256(yol) == beklenen and devam_var and not os.path.exists(dergi_yolu(yol))
    kayit("WAL devam ettirme", gecti,
          f"ilk progress={ilk_progress and ilk_progress.get('downloaded_bytes')} bayt")
    c2.kapat()
    temizle(ad)


def test_liste(cekirdek):
    cekirdek.gonder({"action": "LIST"})
    olay = cekirdek.bekle("LIST_RESULT", zaman_asimi=10)
    kayit("LIST", olay is not None and isinstance(olay.get("jobs"), list))


def main():
    if not os.path.exists(EXE):
        sys.exit("Çekirdek exe'si yok: önce cargo build --release")
    sunucu = subprocess.Popen(
        [sys.executable, os.path.join(KOK, "tests", "test_server.py"), str(PORT)],
        stdout=subprocess.DEVNULL,
    )
    time.sleep(1.0)
    try:
        cekirdek = Cekirdek()
        test_ping(cekirdek)
        test_range_16mb(cekirdek)
        test_hls(cekirdek, "hls_duz", "titan_hls_duz")
        test_hls(cekirdek, "hls_aes", "titan_hls_aes")
        test_dash(cekirdek)
        test_iptal(cekirdek)
        test_liste(cekirdek)
        cekirdek.kapat()
        test_wal_devam()
    finally:
        sunucu.terminate()

    print("\n===== ÖZET =====")
    gecen = sum(1 for _, g, _ in sonuclar if g)
    for ad, gecti, not_ in sonuclar:
        print(f"  {'✔' if gecti else '✘'} {ad}")
    print(f"{gecen}/{len(sonuclar)} test geçti")
    sys.exit(0 if gecen == len(sonuclar) else 1)


if __name__ == "__main__":
    main()
