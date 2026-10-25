# Range destekli yerel test sunucusu — RFC 7233 tek aralık + "yavas_" önekli dosyalarda kısıtlama
import os
import re
import sys
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

KOK = os.path.join(os.path.dirname(os.path.abspath(__file__)), "fixture")
YAVAS_PARCA_GECIKME = 0.12  # sn / 64 KB


class Isleyici(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *args):
        pass

    def do_HEAD(self):
        self._sunum(baslik_sadece=True)

    def do_GET(self):
        self._sunum(baslik_sadece=False)

    def _sunum(self, baslik_sadece):
        yol = self.path.split("?", 1)[0].lstrip("/")
        dosya = os.path.normpath(os.path.join(KOK, yol))
        if not dosya.startswith(KOK) or not os.path.isfile(dosya):
            self.send_error(404)
            return
        boyut = os.path.getsize(dosya)
        aralik = self.headers.get("Range")
        bas, son = 0, boyut - 1
        kod = 200
        if aralik:
            es = re.match(r"bytes=(\d*)-(\d*)$", aralik.strip())
            if not es:
                self.send_error(416)
                return
            bas = int(es.group(1)) if es.group(1) else 0
            son = int(es.group(2)) if es.group(2) else boyut - 1
            if bas > son or bas >= boyut:
                self.send_error(416)
                return
            son = min(son, boyut - 1)
            kod = 206

        self.send_response(kod)
        self.send_header("Accept-Ranges", "bytes")
        self.send_header("Content-Type", self._tur(dosya))
        uzunluk = son - bas + 1
        self.send_header("Content-Length", str(uzunluk))
        if kod == 206:
            self.send_header("Content-Range", f"bytes {bas}-{son}/{boyut}")
        self.end_headers()
        if baslik_sadece:
            return

        yavas = os.path.basename(dosya).startswith("yavas_")
        with open(dosya, "rb") as f:
            f.seek(bas)
            kalan = uzunluk
            while kalan > 0:
                parca = f.read(min(65536, kalan))
                if not parca:
                    break
                try:
                    self.wfile.write(parca)
                except (BrokenPipeError, ConnectionAbortedError, ConnectionResetError):
                    return
                kalan -= len(parca)
                if yavas:
                    time.sleep(YAVAS_PARCA_GECIKME)

    @staticmethod
    def _tur(dosya):
        from urllib.parse import unquote
        ad = dosya.lower()
        return (
            "application/vnd.apple.mpegurl" if ad.endswith(".m3u8")
            else "application/dash+xml" if ad.endswith(".mpd")
            else "video/mp4" if ad.endswith((".mp4", ".m4s"))
            else "video/MP2T" if ad.endswith(".ts")
            else "audio/mpeg" if ad.endswith(".key")
            else "application/octet-stream"
        )


class Sunucu(ThreadingHTTPServer):
    def handle_error(self, request, client_address):
        hata = sys.exc_info()[1]
        if isinstance(hata, (ConnectionResetError, ConnectionAbortedError, BrokenPipeError)):
            return
        super().handle_error(request, client_address)


if __name__ == "__main__":
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8471
    sunucu = Sunucu(("127.0.0.1", port), Isleyici)
    print(f"Test sunucusu http://127.0.0.1:{port} adresinde hazır", flush=True)
    sunucu.serve_forever()
