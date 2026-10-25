# Test sabitlerini üretir: 16 MB deterministik dosya, yavaş dosya, HLS (düz + AES-128), DASH
import os
import subprocess
import sys

KOK = os.path.normpath(os.path.join(os.path.dirname(__file__)))
CIKTI = os.path.join(KOK, "fixture")
KAYNAK_MP4 = os.path.join(CIKTI, "kaynak.mp4")


def kos(cmd):
    r = subprocess.run(cmd, capture_output=True, text=True, shell=True)
    if r.returncode != 0:
        print(r.stderr[-3000:])
        sys.exit(f"Komut başarısız: {cmd}")


def main():
    os.makedirs(CIKTI, exist_ok=True)

    # 1) Deterministik 16 MB dosya
    dosya = os.path.join(CIKTI, "test_16mb.bin")
    blok = bytes(range(256)) * 4096  # 1 MB
    with open(dosya, "wb") as f:
        for i in range(16):
            f.write(bytes([i]) + blok[1:])  # her blok ilk baytı farklı
    print("test_16mb.bin üretildi")

    # 2) Yavaş sunulacak 16 MB dosya (iptal/devam testi için)
    with open(os.path.join(CIKTI, "yavas_16mb.bin"), "wb") as f:
        for i in range(16):
            f.write(bytes([0xAA]) + blok[1:])

    # 3) 6 saniyelik video+ses kaynağı
    if not os.path.exists(KAYNAK_MP4):
        kos(
            f'ffmpeg -y -hide_banner -loglevel error -f lavfi -i testsrc2=size=320x240:rate=15:duration=6 '
            f'-f lavfi -i sine=frequency=440:duration=6 -c:v libx264 -preset ultrafast -crf 30 -g 15 -keyint_min 15 -sc_threshold 0 '
            f'-c:a aac -shortest "{KAYNAK_MP4}"'
        )
    print("kaynak.mp4 üretildi")

    # 4) HLS fMP4 (düz)
    hls_dizin = os.path.join(CIKTI, "hls_duz")
    os.makedirs(hls_dizin, exist_ok=True)
    kos(
        f'ffmpeg -y -hide_banner -loglevel error -i "{KAYNAK_MP4}" -c copy '
        f'-f hls -hls_list_size 0 -hls_segment_type fmp4 '
        f'-hls_time 1 -hls_segment_filename "{hls_dizin}/seg%03d.m4s" "{hls_dizin}/playlist.m3u8"'
    )
    print("hls_duz üretildi")

    # 5) HLS AES-128 (TS)
    hls_aes = os.path.join(CIKTI, "hls_aes")
    os.makedirs(hls_aes, exist_ok=True)
    anahtar = os.path.join(hls_aes, "enc.key")
    with open(anahtar, "wb") as f:
        f.write(bytes(range(16)))
    bilgi = os.path.join(hls_aes, "keyinfo")
    with open(bilgi, "w", encoding="utf-8") as f:
        f.write("http://127.0.0.1:8471/hls_aes/enc.key\n")
        f.write(anahtar.replace("\\", "/") + "\n")
    kos(
        f'ffmpeg -y -hide_banner -loglevel error -i "{KAYNAK_MP4}" -c copy '
        f'-f hls -hls_list_size 0 -hls_time 1 '
        f'-hls_key_info_file "{bilgi}" '
        f'-hls_segment_filename "{hls_aes}/seg%03d.ts" "{hls_aes}/playlist.m3u8"'
    )
    print("hls_aes üretildi")

    # 6) DASH (video + ses, SegmentTemplate süre tabanlı)
    dash_dizin = os.path.join(CIKTI, "dash")
    os.makedirs(dash_dizin, exist_ok=True)
    kos(
        f'ffmpeg -y -hide_banner -loglevel error -i "{KAYNAK_MP4}" -c copy '
        f'-f dash -use_template 1 -use_timeline 0 -seg_duration 1 '
        f'-init_seg_name "init_$RepresentationID$.m4s" '
        f'-media_seg_name "seg_$RepresentationID$_$Number$.m4s" "{dash_dizin}/sunum.mpd"'
    )
    print("dash üretildi")

    print("Tüm sabitler hazır")


if __name__ == "__main__":
    main()
