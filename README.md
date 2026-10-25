# Titanium İndirme Hızlandırıcı

Windows için Manifest V3 tarayıcı uzantısı ile Rust Native Messaging çekirdeğini birleştiren yerel indirme yöneticisi. Medya trafiğini yerelde koklar, HTTP Range indirmelerini paralel yürütür, HLS ve DASH akışlarını çözer; FFmpeg varsa kayıpsız birleştirir.

Ayrıntılı mimari, protokol ve motor belgesi: [`docs/readme.md`](docs/readme.md).

## Özellikler

- Chrome, Edge ve Brave (Chromium 120+) uyumlu MV3 uzantısı; `webRequest` ve MediaSource kancalarıyla medya yakalama, sayfa içi **"⬇ Medyayı İndir"** düğmesi ve sağ tık menüsü
- Çok bağlantılı HTTP Range indirmesi: dinamik iş paylaşımı, kesit bölme ve atomik iş çalma
- Seyrek dosyaya doğrudan ofset yazımı; geçici parça dosyası ve birleştirme aşaması yok
- `.titan_state` jurnalı sayesinde çökme veya kapatma sonrası kaldığı yerden devam
- HLS (AES-128/CBC dahil) ve DASH (SegmentList, SegmentTemplate, SegmentBase) desteği; canlı ve DRM korumalı akışlar açık hatayla reddedilir
- Çerez ve oturum başlıklarının aktarımı; süresi dolan bağlantılar için oturum yenileme (`REFRESH_LINK`)
- Türkçe arayüz ve hata iletileri; uzantı kimliği `manifest.json` anahtarıyla sabittir

## Hazır Uygulama

`release` klasöründeki `Titanium_beta-vYYYYMMDDHHMM.zip` paketi son kullanıcı içindir; Rust, Python veya FFmpeg kurulması gerekmez. Klasörde yalnızca en güncel paket bulundurulur; her derlemede önceki sürümler otomatik olarak kaldırılır. Dosya adındaki `beta-vYYYYMMDDHHMM` damgası derleme anını gösterir; paket adı ile `OKUBENI.txt` başlığı aynı derleme damgasını taşır.

Sürüm paketleri depoda tutulmaz; GitHub Releases üzerinden indirilir. Paketi kaynaktan üretmek için `powershell -File scripts/release_uret.ps1` çalıştırılır.

Kurulum iki adımdır:

1. Paketteki `KURULUM.cmd` dosyasını çalıştırın. Çekirdek ve uzantı `%LOCALAPPDATA%\TitanDownloader` konumuna kopyalanır, Chrome/Edge/Brave için yerel mesajlaşma kaydı yazılır ve Uzantılar sayfası açılır.
2. `chrome://extensions` sayfasında **Geliştirici modu**'nu etkinleştirin → **Paketlenmemiş yükle** → şu klasörü seçin: `%LOCALAPPDATA%\TitanDownloader\extension`

> Zip'in kök klasörünü seçmeyin; `manifest.json` yalnızca `extension` alt klasöründedir. Windows'ta mağaza dışı `.crx` kurulumu Chrome tarafından desteklenmediği için resmî yol geliştirici modudur; sabit anahtar sayesinde uzantı kimliği güncellemede değişmez.

Kaldırma: `KALDIR.cmd` dosyasını çalıştırın, ardından uzantıyı `chrome://extensions` sayfasından kaldırın.

## Kaynaktan Derleme

Gereksinimler: Rust 1.85+ (edition 2024, MSVC hedefi) ve MSVC C++ derleyicisi (Visual Studio Build Tools).

```powershell
cd core
cargo build --release
cd ..
powershell -File scripts/release_uret.ps1
```

Çekirdek `core/target/release/titan-downloader-core.exe` olarak derlenir. Paketleme betiği sürüm dizinini ve zip'i `release/` altında üretir ve exe/zip SHA-256 özetlerini yazar. FFmpeg'li tam paket için `-FfmpegYolu "C:\...\ffmpeg.exe"` parametresi kullanılır.

## Geliştirici Kurulumu

Gereksinimler: Python 3, FFmpeg/ffprobe (test sabitleri ve HLS/DASH doğrulaması için zorunlu), OpenSSL (yalnızca kimlik yenilemede).

```powershell
python tests/fixture_uret.py           # test sabitleri (depoda tutulmaz; FFmpeg gerekir)
cargo test --manifest-path core/Cargo.toml
python tests/test_nm.py                # 8 uçtan uca Native Messaging testi (ffprobe gerekir)
python tests/release_simulasyon.py     # paketi son kullanıcı gibi kurup doğrular
python scripts/make_key.py             # isteğe bağlı: uzantı kimliğini yenilemek için
```

- `make_key.py` yalnızca uzantı kimliğini ilk kez üretmek veya yenilemek içindir; normal geliştirmede gerekmez. Depodaki `manifest.json` `key` alanı kimliği sabitler, mevcut özel anahtar korunur ve anahtar depoya eklenmez.
- `fixture_uret.py` 16 MB'lık sabit test dosyası ile HLS (düz/AES-128) ve DASH sabitlerini üretir; FFmpeg gerektirir.
- `release_simulasyon.py` çalıştırılmadan önce `release_uret.ps1` ile paket üretilmiş olmalıdır.
- Yerel host kaydı için: `powershell -ExecutionPolicy Bypass -File scripts/install.ps1 -Sessiz`

## Güvenlik ve Gizlilik

- Tüm indirme ve ayrıştırma işlemleri yerelde çalışır; telemetri, uzak günlük veya bulut aktarımı yoktur. Ağ erişimi indirme hedefi ve oturum yenileme ile sınırlıdır.
- Tercihler ve yakalanan medya listesi `chrome.storage.local` içinde tutulur; devam jurnalı hedef klasördeki `.titan_state` dosyasındadır.
- Yerel mesajlaşma kaydı yalnızca `HKCU` altına yazılır; yönetici yetkisi gerekmez. Host bildirimi kurulum sırasında `install.ps1` tarafından üretilir.
- Yalnızca `http(s)` adresleri kabul edilir; dosya adları Windows'a uygun biçimde temizlenir. DRM korumalı akışlar tespit edilip reddedilir.
- `host/titan_key.pem`, uzantı kimliğini sabitleyen özel anahtardır; depoda tutulmaz ve paylaşılmamalıdır.

## Sürümlandırma

Paket sürümü, üretim anındaki yerel saatten türetilen `beta-vYYYYMMDDHHMM` damgasıdır; dosya adı `Titanium_beta-vYYYYMMDDHHMM.zip` biçimindedir. Uzantı bildirimi Chrome'un sayısal sürüm zorunluluğu nedeniyle `extension/manifest.json` içinde `1.0.0`, çekirdek crate sürümü `core/Cargo.toml` içinde `0.1.0` olarak tanımlıdır.

## Depo Yapısı

| Yol | İçerik | Depoda |
|---|---|---|
| `README.md`, `.gitignore` | Giriş belgesi ve yok sayma kuralları | İzlenir |
| `extension/` | MV3 manifest, service worker, content script'ler, popup ve ikonlar | İzlenir |
| `core/` | Rust çekirdeği: `Cargo.toml`, `Cargo.lock`, `src/` | İzlenir |
| `scripts/` | Kurulum, kaldırma, anahtar/ikon üretimi ve paketleme betikleri (`.ps1`, `.py`) | İzlenir |
| `tests/` | Uçtan uca testler ve yerel test sunucusu (`.py`) | İzlenir |
| `docs/` | Ayrıntılı teknik belge (`docs/readme.md`) | İzlenir |
| `core/target/`, `release/`, `tests/fixture/` | Derleme çıktısı, sürüm paketleri, üretilen test sabitleri | Tutulmaz |
| `host/` | Yerel özel anahtar (`titan_key.pem`) | Tutulmaz |
| `AGENTS.md`, `CLAUDE.md`, `.claude/`, `.codegraph/`, `.gitnexus/` | Yerel ajan/araç yapılandırmaları | Tutulmaz |

## Lisans

Bu depoda ayrı bir `LICENSE` dosyası bulunmuyor. Çekirdek, `core/Cargo.toml` içinde tanımlanan üçüncü taraf crate'leri kullanır; bu bağımlılıklar ağırlıklı olarak MIT ve/veya Apache-2.0 lisanslıdır, azınlıkta ISC, BSD-3-Clause ve MPL-2.0 gibi lisanslar da bulunur. Yeniden dağıtımda bağımlılık lisansları dikkate alınmalıdır.

---

© 2026 SP GROUP ENERJİ SAN. VE TİC. LTD. ŞTİ. — info@spgroupenerji.com
