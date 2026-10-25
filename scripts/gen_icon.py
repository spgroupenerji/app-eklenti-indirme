# Titanium ikon üretici — harici bağımlılık yok (zlib + struct)
import os
import struct
import zlib

KOK = os.path.join(os.path.dirname(__file__), "..", "extension", "icons")


def png_yaz(yol, pikseller, boyut):
    ham = b"".join(b"\x00" + satir for satir in pikseller)
    def parca(tip, veri):
        return struct.pack(">I", len(veri)) + tip + veri + struct.pack(">I", zlib.crc32(tip + veri) & 0xFFFFFFFF)
    ihdr = struct.pack(">IIBBBBB", boyut, boyut, 8, 6, 0, 0, 0)
    icerik = (b"\x89PNG\r\n\x1a\n" + parca(b"IHDR", ihdr)
              + parca(b"IDAT", zlib.compress(ham, 9)) + parca(b"IEND", b""))
    with open(yol, "wb") as f:
        f.write(icerik)


def ciz(boyut):
    # 4x süperörnekleme ile pürüzsüz kenar
    s = boyut * 4
    zemin = (13, 17, 23, 255)
    kenar = (48, 54, 61, 255)
    ok = (88, 166, 255, 255)
    cizgi = (31, 111, 235, 255)

    def yuvarlak_kare(x, y, r):
        # köşeleri yuvarlatılmış kare içinde mi
        en_yakin_x = min(max(x, r), s - r - 1)
        en_yakin_y = min(max(y, r), s - r - 1)
        dx, dy = x - en_yakin_x, y - en_yakin_y
        return dx * dx + dy * dy <= r * r

    def ok_uzerinde(x, y):
        # Aşağı ok: gövde + üçgen baş
        govde_ust = int(s * 0.16)
        govde_alt = int(s * 0.52)
        govde_kenar = int(s * 0.155)
        merkez = s / 2
        if govde_ust <= y <= govde_alt and abs(x - merkez) <= govde_kenar:
            return True
        bas_ust = govde_alt
        bas_alt = int(s * 0.82)
        if bas_ust < y <= bas_alt:
            yayilim = govde_kenar + (bas_alt - bas_ust) * 0.95 * ((y - bas_ust) / (bas_alt - bas_ust))
            if abs(x - merkez) <= yayilim:
                return True
        return False

    def taban_cizgisi(x, y):
        ust = int(s * 0.88)
        kalinlik = max(1, int(s * 0.045))
        return ust <= y < ust + kalinlik and int(s * 0.18) <= x <= int(s * 0.82)

    pikseller = []
    yari_kenar = max(1, int(s * 0.02))
    for y0 in range(boyut):
        satir = bytearray()
        for x0 in range(boyut):
            r, g, b, a, alfa = 0, 0, 0, 0, 0.0
            for dy in range(4):
                for dx in range(4):
                    x = x0 * 4 + dx
                    y = y0 * 4 + dy
                    if not yuvarlak_kare(x, y, int(s * 0.18)):
                        continue
                    alfa += 1
                    if ok_uzerinde(x, y):
                        renk = ok
                    elif taban_cizgisi(x, y):
                        renk = cizgi
                    elif (min(x, s - 1 - x) < yari_kenar or min(y, s - 1 - y) < yari_kenar):
                        renk = kenar
                    else:
                        renk = zemin
                    r += renk[0]; g += renk[1]; b += renk[2]; a += renk[3]
            n = max(alfa, 1)
            kirmizi = int(r / 16) if alfa else 0
            yesil = int(g / 16) if alfa else 0
            mavi = int(b / 16) if alfa else 0
            alfa_kanal = int(a / 16) if alfa else 0
            satir += bytes((kirmizi, yesil, mavi, alfa_kanal))
        pikseller.append(bytes(satir))
    return pikseller


os.makedirs(KOK, exist_ok=True)
for boyut in (16, 32, 48, 128):
    png_yaz(os.path.join(KOK, f"{boyut}.png"), ciz(boyut), boyut)
    print(f"{boyut}.png üretildi")
