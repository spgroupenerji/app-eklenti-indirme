# Sürüm paketini son kullanıcı gibi kurar ve uçtan uca doğrular
import atexit
import json
import os
import shutil
import struct
import subprocess
import tempfile
import zipfile

KOK = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
RELEASE = os.path.join(KOK, "release")
if not os.path.isdir(RELEASE):
    raise SystemExit("Sürüm zip'i bulunamadı; önce scripts/release_uret.ps1 çalıştırın")
adaylar = [
    os.path.join(RELEASE, ad)
    for ad in os.listdir(RELEASE)
    if ad.startswith("Titanium") and ad.endswith(".zip") and not ad.endswith("-tam.zip")
]
if not adaylar:
    raise SystemExit("Sürüm zip'i bulunamadı; önce scripts/release_uret.ps1 çalıştırın")
ZIP = max(adaylar, key=os.path.getmtime)
TEST_KOK = os.path.join(tempfile.gettempdir(), "TitaniumReleaseTest")
KURULUM = os.path.join(TEST_KOK, "kurulum")
ACILAN = os.path.join(TEST_KOK, "acilan")

REG_SORGU = (
    "(Get-ItemProperty 'HKCU:\\Software\\Google\\Chrome\\NativeMessagingHosts"
    "\\com.titan.downloader').'(default)'"
)

REG_ANAHTARLARI = (
    r"HKCU:\Software\Google\Chrome\NativeMessagingHosts\com.titan.downloader",
    r"HKCU:\Software\Microsoft\Edge\NativeMessagingHosts\com.titan.downloader",
    r"HKCU:\Software\BraveSoftware\Brave-Browser\NativeMessagingHosts\com.titan.downloader",
)


def adim(n, metin):
    print(f"[{n}] {metin}")


def kayit_oku():
    durum = {}
    for anahtar in REG_ANAHTARLARI:
        komut = (
            f"if (Test-Path '{anahtar}') {{ (Get-ItemProperty '{anahtar}').'(default)' }}"
            " else { '__YOK__' }"
        )
        r = subprocess.run(
            ["powershell", "-NoProfile", "-Command", komut],
            capture_output=True, text=True, encoding="utf-8", errors="replace",
        )
        durum[anahtar] = r.stdout.strip()
    return durum


def kayit_geri_yukle(durum):
    for anahtar, deger in durum.items():
        if deger == "__YOK__":
            komut = f"if (Test-Path '{anahtar}') {{ Remove-Item '{anahtar}' -Recurse -Force }}"
            subprocess.run(["powershell", "-NoProfile", "-Command", komut], capture_output=True)
            continue
        komut = (
            f"New-Item -Path '{anahtar}' -Force | Out-Null; "
            "Set-ItemProperty -Path $env:TITAN_REG_ANAHTAR -Name '(default)' -Value $env:TITAN_REG_DEGER"
        )
        ortam = os.environ.copy()
        ortam["TITAN_REG_ANAHTAR"] = anahtar
        ortam["TITAN_REG_DEGER"] = deger
        subprocess.run(
            ["powershell", "-NoProfile", "-Command", komut],
            capture_output=True, env=ortam,
        )


def main():
    if os.path.exists(TEST_KOK):
        shutil.rmtree(TEST_KOK)
    os.makedirs(TEST_KOK)

    kayit_yedegi = kayit_oku()
    atexit.register(kayit_geri_yukle, kayit_yedegi)

    adim(1, "Zip açılıyor (son kullanıcı indirmesi simülasyonu)")
    with zipfile.ZipFile(ZIP) as z:
        z.extractall(ACILAN)

    adim(2, "install.ps1 sessiz kipte çalıştırılıyor")
    r = subprocess.run(
        ["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass",
         "-File", os.path.join(ACILAN, "install.ps1"),
         "-KurulumDizini", KURULUM, "-Sessiz"],
        capture_output=True, text=True, encoding="utf-8", errors="replace",
    )
    print("\n".join("    " + s for s in r.stdout.strip().splitlines()))
    assert r.returncode == 0, r.stderr
    assert "Kurulum tamamlandı" in r.stdout, "Türkçe çıktı bozuk olabilir"

    adim(3, "Kurulu yapı doğrulanıyor")
    exe = os.path.join(KURULUM, "titan-downloader-core.exe")
    ext = os.path.join(KURULUM, "extension", "manifest.json")
    host = os.path.join(KURULUM, "com.titan.downloader.json")
    for yol in (exe, ext, host):
        assert os.path.isfile(yol), f"eksik: {yol}"
    veri = open(host, "rb").read()
    assert not veri.startswith(b"\xef\xbb\xbf"), "host manifest BOM'lu!"
    hj = json.loads(veri.decode("utf-8"))
    assert hj["path"] == exe, hj["path"]
    assert hj["type"] == "stdio"
    origin = hj["allowed_origins"][0]
    manifest = json.load(open(ext, encoding="utf-8"))
    assert "key" in manifest, "kopyalanan uzantıda anahtar yok"
    for rel in (
        manifest["background"]["service_worker"],
        manifest["action"]["default_popup"],
        *[j for s in manifest.get("content_scripts", []) for j in s.get("js", [])],
        *manifest.get("icons", {}).values(),
    ):
        assert os.path.isfile(os.path.join(os.path.dirname(ext), rel)), f"manifest hedefi eksik: {rel}"
    print(f"    host manifest BOM'suz, yol doğru, origin={origin}")

    adim(4, "Kayıt defteri hedefi doğrulanıyor")
    r = subprocess.run(["powershell", "-NoProfile", "-Command", REG_SORGU],
                       capture_output=True, text=True)
    assert r.stdout.strip() == host, r.stdout
    print("    kayıt defteri ->", r.stdout.strip())

    adim(5, "Kurulu çekirdekle NM PING testi")
    p = subprocess.Popen([exe], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                         stderr=subprocess.DEVNULL)
    mesaj = json.dumps({"action": "PING"}).encode()
    p.stdin.write(struct.pack("<I", len(mesaj)) + mesaj)
    p.stdin.flush()
    bas = p.stdout.read(4)
    assert len(bas) == 4, "yanıt yok"
    (n,) = struct.unpack("<I", bas)
    yanit = json.loads(p.stdout.read(n))
    p.kill()
    assert yanit["status"] == "PONG", yanit
    print("    PONG alındı")

    adim(6, "OKUBENI.txt kodlaması")
    okubeni = open(os.path.join(ACILAN, "OKUBENI.txt"), "rb").read()
    assert okubeni.startswith(b"\xef\xbb\xbf"), "OKUBENI BOM'suz"
    metin = okubeni.decode("utf-8-sig")
    assert "İNDİRME HIZLANDIRICI" in metin, "Türkçe karakterler bozuk"
    assert "UZANTI-YUKLE.cmd" in metin, "kolay yükleme yönergesi yok"
    assert "Manifest dosyası eksik" in metin, "hata giderme yönergesi yok"
    assert "SEÇMEYİN" in metin and "extension" in metin, "kök klasör uyarısı yok"
    print("    UTF-8 BOM + Türkçe doğru")

    adim(7, "Cmd başlatıcılar BOM'suz ve CRLF")
    for ad in ("KURULUM.cmd", "KALDIR.cmd", "UZANTI-YUKLE.cmd"):
        v = open(os.path.join(ACILAN, ad), "rb").read()
        assert not v.startswith(b"\xef\xbb\xbf"), ad
        assert b"\r\n" in v and b"powershell" in v
    print("    cmd dosyaları geçerli")

    adim(8, "UZANTI-YUKLE yardımcısı sessiz kipte doğrulanıyor")
    r = subprocess.run(
        ["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass",
         "-File", os.path.join(ACILAN, "uzanti_yukle.ps1"),
         "-KurulumDizini", KURULUM, "-Sessiz"],
        capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=30,
    )
    assert r.returncode == 0, r.stderr
    assert "UZANTIYI YÜKLEME" in r.stdout and "extension" in r.stdout, r.stdout
    print("    doğru klasör yolu ve yönerge basıldı")

    adim(9, "Kurulmamış uzantı için yardımcı hata veriyor")
    bos = os.path.join(TEST_KOK, "bos")
    os.makedirs(bos)
    r = subprocess.run(
        ["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass",
         "-File", os.path.join(ACILAN, "uzanti_yukle.ps1"),
         "-KurulumDizini", bos, "-Sessiz"],
        capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=30,
    )
    assert r.returncode == 1, r.returncode
    assert "KURULUM.cmd" in r.stdout, r.stdout
    print("    eksik kurulumda KURULUM.cmd yönlendirmesi doğru")

    print("\nSON KULLANICI SİMÜLASYONU: TÜM KONTROLLER GEÇTİ")
    print(f"Geçici kurulum: {KURULUM}")


if __name__ == "__main__":
    main()
