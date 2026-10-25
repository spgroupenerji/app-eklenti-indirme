# Kararlı uzantı kimliği üretir: RSA-2048 anahtar -> manifest "key" + uzantı ID'si
# Kullanım: python scripts/make_key.py
# Özel anahtar host/titan_key.pem dosyasında yerelde saklanır; depoya girmez.
import base64
import hashlib
import json
import os
import subprocess

KOK = os.path.normpath(os.path.join(os.path.dirname(__file__), ".."))
KEY_DOSYA = os.path.join(KOK, "host", "titan_key.pem")
MANIFEST = os.path.join(KOK, "extension", "manifest.json")


def uzanti_id(pub_der: bytes) -> str:
    ozet = hashlib.sha256(pub_der).digest()[:16]
    alfabe = "abcdefghijklmnop"
    return "".join(alfabe[b >> 4] + alfabe[b & 0xF] for b in ozet)


def anahtar_uret() -> None:
    if os.path.exists(KEY_DOSYA):
        return
    with open(MANIFEST, encoding="utf-8") as f:
        mevcut_key = json.load(f).get("key")
    if mevcut_key:
        raise SystemExit(
            "host/titan_key.pem bulunamadı ancak manifest.json içinde 'key' var. "
            "Kimliği korumak için özel anahtarı geri yükleyin; bilinçli kimlik yenilemesi "
            "için önce manifest.json içindeki 'key' alanını silin."
        )
    try:
        subprocess.run(
            ["openssl", "genrsa", "-out", KEY_DOSYA, "2048"],
            check=True, capture_output=True,
        )
    except FileNotFoundError:
        raise SystemExit("openssl bulunamadı; anahtar üretimi için OpenSSL gerekir.")


def main():
    os.makedirs(os.path.dirname(KEY_DOSYA), exist_ok=True)
    anahtar_uret()
    der = subprocess.run(
        ["openssl", "rsa", "-in", KEY_DOSYA, "-pubout", "-outform", "DER"],
        check=True, capture_output=True,
    ).stdout
    kimlik = uzanti_id(der)

    with open(MANIFEST, encoding="utf-8") as f:
        manifest = json.load(f)
    manifest["key"] = base64.b64encode(der).decode("ascii")
    with open(MANIFEST, "w", encoding="utf-8", newline="\n") as f:
        json.dump(manifest, f, indent=2, ensure_ascii=False)
        f.write("\n")

    print(f"Uzantı kimliği: {kimlik}")
    print("manifest.json 'key' alanı yazıldı; özel anahtar host/titan_key.pem (depoya girmez)")


if __name__ == "__main__":
    main()
