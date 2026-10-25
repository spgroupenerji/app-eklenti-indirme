# Titanium İndirme Hızlandırıcı

Manifest V3 tarayıcı uzantısı ile Rust Native Messaging çekirdeğini birleştiren yerel indirme yöneticisi. Ağ trafiğini pasif olarak koklar, HTTP Range indirmelerini paralel yürütür, HLS/DASH akışlarını çözer ve FFmpeg varsa kayıpsız olarak tek dosyada birleştirir.

> **Sürüm:** `beta-vYYYYMMDDHHMM` (paket) · uzantı bildirimi: 1.0.0 · çekirdek crate: 0.1.0 · **Paket adı:** `Titanium_beta-vYYYYMMDDHHMM.zip`
> **Birincil platform:** Windows x64 · **Tarayıcı:** Chrome/Edge/Brave (Chromium 120+)
> Bu belge kaynak kodu esas alır; davranış iddiaları `core/`, `extension/` ve `scripts/` içindeki uygulamayla eşleşir.

---

## İçindekiler

- [Genel Bakış](#genel-bakış)
- [Öne Çıkan Yetenekler](#öne-çıkan-yetenekler)
- [Mimari](#mimari)
- [Bileşenler ve Dizin Yapısı](#bileşenler-ve-dizin-yapısı)
- [Son Kullanıcı Kurulumu](#son-kullanıcı-kurulumu)
- [Kullanım](#kullanım)
- [Geliştirme](#geliştirme)
- [Sürümleme ve Paketleme](#sürümleme-ve-paketleme)
- [Native Messaging Protokolü](#native-messaging-protokolü)
- [Çekirdek Motoru](#çekirdek-motoru)
- [HLS ve DASH](#hls-ve-dash)
- [Dayanıklılık ve Devam Ettirme](#dayanıklılık-ve-devam-ettirme)
- [Güvenlik ve Gizlilik](#güvenlik-ve-gizlilik)
- [Bilinçli Sınırlar](#bilinçli-sınırlar)
- [Sorun Giderme](#sorun-giderme)
- [Test ve Doğrulama](#test-ve-doğrulama)

---

## Genel Bakış

Titanium iki süreçten oluşur:

1. **Tarayıcı uzantısı (MV3):** Sayfa trafiğini ve MediaSource kullanımını izler, indirme isteğini çerezler ve oturum başlıklarıyla birlikte yerel çekirdeğe iletir, ilerlemeyi popup üzerinde gösterir.
2. **Rust çekirdeği (Native Messaging host):** HTTP Range, HLS ve DASH indirmelerini yürütür; seyrek dosyaya doğrudan ofset yazar; yarıda kalan indirmeleri jurnalden sürdürür; FFmpeg varsa kayıpsız remux yapar.

Tüm veri akışı yereldir. Uzantı ile çekirdek arasında `chrome.runtime.connectNative("com.titan.downloader")` üzerinden stdio tabanlı ikili çerçeveleme kullanılır. Harici bir sunucu, bulut servisi veya telemetri yoktur.

| Katman | Teknoloji | Görev |
|---|---|---|
| Uzantı | JavaScript, Manifest V3 | Ağ/MSE koklama, oturum bilgisi, kullanıcı arayüzü |
| Çekirdek | Rust 2024, Tokio, reqwest | Paralel indirme, HLS/DASH, jurnal, remux |
| Paketleme | PowerShell 5.1, Python 3 | Kurulum, kayıt defteri, sürüm zip'i, testler |
| Medya | FFmpeg (isteğe bağlı) | Kayıpsız remux ve video+ses birleştirme |

---

## Öne Çıkan Yetenekler

- **Dinamik iş paylaşımı:** Bekleyen en büyük kesit yarıya bölünür; boş kalan işçi, aktif kesitlerin en büyüğünün ortasından iş çalar.
- **Doğrudan ofset yazımı:** Geçici parça dosyası yoktur. Windows'ta `FSCTL_SET_SPARSE` + `seek_write`, Unix'te `fallocate` + `write_all_at` kullanılır.
- **Yeniden başlatmaya dayanıklılık:** Tamamlanan aralıklar `.titan_state` jurnalına `fsync` ile yazılır; süreç ölürse indirme kaldığı yerden sürer.
- **Bağlantı yenileme:** Sunucu `401/403/410` döndürürse uzantı kaynak sayfayı tazeler, çerezleri toplar ve `REFRESH_LINK` ile çekirdeğe yeni oturum verir.
- **HLS:** En yüksek bant genişliği/çözünürlüklü varyant seçilir; AES-128 (CBC/PKCS7) hat üzerinde çözülür; `EXT-X-MAP` ve byte-range desteklenir.
- **DASH:** SegmentList, SegmentTemplate (SegmentTimeline veya süre tabanlı) ve SegmentBase desteklenir; en iyi video ve ses temsilleri ayrı ayrı indirilir.
- **Kayıpsız birleştirme:** FFmpeg bulunursa HLS akışı boru üzerinden MP4'e, DASH video+ses tek MP4'e `-c copy` ile aktarılır.
- **Sabit uzantı kimliği:** `manifest.json` içindeki anahtar sayesinde kimlik `hhpiommjmnandklbedpjhhclpfmlnlih` olarak sabittir; güncellemede değişmez.
- **Kullanıcı dostu hata yönetimi:** Türkçe hata iletileri; DRM ve canlı yayın gibi desteklenmeyen durumlar sessizce yutulmaz, açıkça reddedilir.

---

## Mimari

```text
┌──────────────────────────── Tarayıcı ────────────────────────────┐
│  Sayfa (MAIN world)                                              │
│    mse_hook.js  ──postMessage──▶  bridge.js (ISOLATED world)     │
│                                        │ chrome.runtime.sendMessage
│                                        ▼                         │
│                          background.js (MV3 Service Worker)      │
│             webRequest · cookies · storage · alarms · contextMenus│
└────────────────────────────────────┬─────────────────────────────┘
                                     │ connectNative (stdio)
                                     ▼
                     com.titan.downloader (host manifest)
                                     │ 4 bayt LE uzunluk + JSON
                                     ▼
┌────────────────────────── Rust çekirdeği ────────────────────────┐
│  nm.rs → protocol.rs → job.rs → engine.rs                        │
│      │                    │                                      │
│      │                    ├── probe.rs   (HEAD / Range 0-0)      │
│      │                    ├── hls.rs     (m3u8 + AES-128)        │
│      │                    ├── dash.rs    (MPD + Segment*)        │
│      │                    ├── storage.rs (sparse + jurnal)       │
│      │                    └── remux.rs   (ffmpeg, isteğe bağlı)  │
│      ▼                                                           │
│  %USERPROFILE%\Downloads                                         │
└──────────────────────────────────────────────────────────────────┘
```

**Akış özeti:** Uzantı `START_DOWNLOAD` gönderir → çekirdek hedefi `HEAD` (gerekirse `Range: bytes=0-0`) ile keşfeder → türü HTTP/HLS/DASH olarak sınıflandırır → uygun motoru çalıştırır → `PROGRESS` olayları 500 ms aralıkla akar → iş `COMPLETED`, `FAILED` veya `CANCELED` ile kapanır.

---

## Bileşenler ve Dizin Yapısı

```text
app-indirme/
├─ extension/
│  ├─ manifest.json              MV3 tanımı, sabit anahtar, izinler
│  ├─ background.js              Native port, webRequest koklama, komutlar
│  ├─ content/
│  │  ├─ mse_hook.js             MAIN world: MediaSource kancaları
│  │  └─ bridge.js               ISOLATED world: postMessage ↔ chrome.runtime
│  ├─ popup/                     popup.html, popup.css, popup.js
│  └─ icons/                     16/32/48/128 px
├─ core/
│  ├─ Cargo.toml                 Rust bağımlılıkları ve release profili
│  └─ src/
│     ├─ main.rs                 Tokio girişi, komut/olay kanalları
│     ├─ nm.rs                   Native Messaging çerçeveleme (stdin/stdout)
│     ├─ protocol.rs             Command ve Event tipleri
│     ├─ job.rs                  İş kayıtları, hız/ETA izleyici, yol seçimi
│     ├─ engine.rs               Paralel/tek akış motoru, iş paylaşımı
│     ├─ probe.rs                HEAD + Range 0-0 keşfi, tür sınıflandırma
│     ├─ storage.rs              Seyrek dosya, `.titan_state` jurnalı
│     ├─ hls.rs                  m3u8, varyant seçimi, AES-128
│     ├─ dash.rs                 MPD ayrıştırma, SegmentTemplate/List/Base
│     ├─ remux.rs                FFmpeg bulma, boru ve mux işlemleri
│     └─ util.rs                 Sabitler, ad temizleme, hash, süre ayrıştırma
├─ host/
│  └─ titan_key.pem              Yerel özel anahtar (depoda tutulmaz)
├─ scripts/
│  ├─ install.ps1                Kurulum + HKCU kayıtları
│  ├─ uninstall.ps1              Kaldırma
│  ├─ uzanti_yukle.ps1           Doğru uzantı klasörünü açar
│  ├─ release_uret.ps1           Derleme + beta-v paketleme
│  ├─ make_key.py                manifest anahtarı + yerel özel anahtar
│  └─ gen_icon.py                Simge üretimi
├─ tests/
│  ├─ fixture_uret.py            Test sabitleri (16 MB, HLS, DASH)
│  ├─ test_server.py             Yerel HTTP sunucusu (port 8471)
│  ├─ test_nm.py                 8 uçtan uca Native Messaging testi
│  ├─ release_simulasyon.py      Zip'i son kullanıcı gibi kurar (9 adım)
│  ├─ tek_surus.py               Manuel tek indirme sürücüsü
│  └─ fixture/                   Üretilen test sabitleri (depoda tutulmaz)
└─ release/                      Üretilen sürüm paketleri (depoda tutulmaz)
```

---

## Son Kullanıcı Kurulumu

Son kullanıcı Rust, Python veya FFmpeg kurmak zorunda değildir; çekirdek derlenmiş olarak pakette gelir.

1. GitHub Releases'ten indirdiğiniz `Titanium_beta-vYYYYMMDDHHMM.zip` dosyasını bir klasöre açın.
2. `KURULUM.cmd` dosyasına çift tıklayın. Bu adım:
   - çekirdeği ve uzantıyı `%LOCALAPPDATA%\TitanDownloader` konumuna kopyalar,
   - `com.titan.downloader.json` host bildirimini BOM'suz UTF-8 olarak yazar,
   - Chrome, Edge ve Brave için `HKCU` altına yerel mesajlaşma kaydını yazar,
   - `chrome://extensions` sayfasını açar.
3. Açılan sayfada **Geliştirici modu**'nu açın, **Paketlenmemiş yükle** düğmesine basın ve şu klasörü seçin:

   ```text
   %LOCALAPPDATA%\TitanDownloader\extension
   ```

4. İsterseniz `UZANTI-YUKLE.cmd` dosyasını çalıştırın: doğru klasörü Explorer'da açar, yolu panoya kopyalar ve Uzantılar sayfasını açar.

> **Önemli:** Zip'in kök klasörünü veya `TitanDownloader` klasörünü seçmeyin. `manifest.json` yalnızca `extension` alt klasöründedir. Yanlış seçimde Chrome "Manifest dosyası eksik veya okunamıyor" hatası verir.

**Kaldırma:** `KALDIR.cmd` dosyasını çalıştırın; ardından uzantıyı `chrome://extensions` üzerinden kaldırın.

**Neden geliştirici modu?** Windows'ta mağaza dışı `.crx` kurulumu Chrome tarafından desteklenmez. Sabit anahtar sayesinde uzantı kimliği değişmediği için yerel mesajlaşma izni güncellemelerde korunur.

---

## Kullanım

### Medya yakalama ve indirme

- Video/ses içeren sayfalarda sağ üstte **"⬇ Medyayı İndir"** düğmesi belirir. Düğme 45 saniye sonra gizlenir; yeni bir medya yanıtı geldiğinde tekrar görünür.
- Bağlantı, video veya ses üzerinde sağ tık → **"Titanium ile indir"**.
- Popup'ta **Yakalanan Medya** listesi son 20 kaydı gösterir; toplamda en son 100 kayıt saklanır. **Temizle** listeyi boşaltır.
- MSE (`blob:`) akışları yalnızca metaveri olarak listelenir. Blob verisi sayfa belleğinde yaşadığı için doğrudan indirilemez; listedeki düğme bu nedenle devre dışıdır. Sayfadaki indirme düğmesi veya bağlam menüsü kullanılmalıdır.

### Aktif indirmeler

Popup'ta her iş için dosya adı, durum, ilerleme çubuğu, indirilen/toplam boyut, hız, kalan süre ve varsa parça bilgisi gösterilir. Düğmeler:

| Düğme | Etki |
|---|---|
| ⏸ | `PAUSE`: yeni pencere/parça alımı durur; açık bağlantı kısa sürede kapanır |
| ▶ | `RESUME`: aynı iş kimliğiyle indirme devam eder |
| ✖ | `CANCEL`: iş iptal edilir; kısmi dosya ve jurnal silinmez |
| ✕ | `IS_SIL`: tamamlanan/duraklatılan iş kaydı listeden silinir; indirilen dosyaya dokunulmaz |

**Eşlik (concurrency):** Popup'tan `4, 8, 16, 24, 32` seçilebilir; varsayılan `16`'dır. Çekirdek değeri her durumda `1..32` aralığına kısar. Ayar `chrome.storage.local` içinde `ayarlar.eslik` olarak saklanır.

**Bildirimler:** Tamamlanan ve başarısız işler için masaüstü bildirimi gösterilir. Araç çubuğu rozetinde etkin iş sayısı yer alır.

### Kalite seçimi

HLS'de ana oynatma listesindeki en yüksek `(bandwidth, çözünürlük alanı)` varyantı; DASH'te en yüksek `(genişlik × yükseklik, bandwidth)` video temsili ve en yüksek `bandwidth` ses temsili otomatik seçilir. Arayüzde manuel kalite seçimi yoktur.

### Çıktı biçimleri

| Kaynak | FFmpeg var | FFmpeg yok |
|---|---|---|
| HTTP dosya | Doğrudan hedef dosya | Doğrudan hedef dosya |
| HLS (TS) | `.mp4` (kayıpsız remux) | `.ts` |
| HLS (fMP4 / `EXT-X-MAP`) | `.mp4` (kayıpsız remux) | `.mp4` (init + parçalar birleşik) |
| DASH (video+ses) | Tek `.mp4` | `.video.mp4` + `.audio.m4a` ayrı |
| DASH (yalnızca video) | Tek `.mp4` | `.video.mp4` |
| DASH (yalnızca ses) | `.audio.m4a` | `.audio.m4a` |

HTTP indirmeleri `%USERPROFILE%\Downloads` dizinine yazılır. Aynı ad varsa `ad (1).ext`, `ad (2).ext` biçiminde benzersiz yol üretilir. Devam ettirilebilir bir jurnal varsa aynı yol yeniden kullanılır. HLS/DASH çıktıları aynı adla üretildiğinde mevcut dosyanın üzerine yazar.

---

## Geliştirme

### Gereksinimler

- Windows 10/11 x64 (birincil hedef)
- Rust 1.85+ (edition 2024)
- Python 3 (testler ve yardımcı betikler)
- OpenSSL (`make_key.py` ile anahtar üretimi için)
- PowerShell 5.1+
- Chrome/Edge/Brave (Chromium 120+)
- FFmpeg + ffprobe (HLS/DASH birleştirme için isteğe bağlı; test sabitleri ve uçtan uca testler için zorunlu)

### Derleme ve test

```powershell
# Test sabitlerini üret (tests/fixture depoda tutulmaz; ffmpeg ister)
python tests/fixture_uret.py

# Çekirdeği derle (çıktı: core/target/release/titan-downloader-core.exe)
cd core
cargo build --release
cargo test
cd ..

# 8 uçtan uca Native Messaging testi (yerel sunucu port 8471; ffprobe ister)
python tests/test_nm.py

# Manuel tek indirme: URL, dosya adı, eşlik
python tests/tek_surus.py "https://ornek/dosya.bin" manuel.bin 8
```

### Uzantıyı yerelde yükleme

```powershell
# Çekirdeği ve uzantıyı LOCALAPPDATA'ya kur, HKCU kayıtlarını yaz (tarayıcı açmadan)
powershell -ExecutionPolicy Bypass -File scripts/install.ps1 -Sessiz
```

Ardından `chrome://extensions` → Geliştirici modu → Paketlenmemiş yükle → `%LOCALAPPDATA%\TitanDownloader\extension`. Depodaki `extension/` klasörünü doğrudan yüklemek de mümkündür; sabit anahtar sayesinde kimlik aynı kalır, ancak host kaydının kurulu olması gerekir.

### Sabit uzantı kimliği

```powershell
python scripts/make_key.py   # manifest "key" alanını yazar; mevcut host/titan_key.pem dosyasını korur
```

`make_key.py` yalnızca manifest `key` alanını ve yerel özel anahtarı yönetir; anahtar yoksa OpenSSL ile RSA-2048 üretir. `host/titan_key.pem` depoda tutulmaz. `install.ps1`, `manifest.json` içindeki `key` alanından SHA-256'nın ilk 16 baytını alıp kimliği hesaplar, host bildirimini çalışma anında BOM'suz UTF-8 olarak üretir ve `allowed_origins` alanına yazar. Anahtar değiştirilirse kimlik de değişir; bu durumda kurulumu yeniden çalıştırmak gerekir. Özel anahtar paylaşılmamalıdır.

---

## Sürümleme ve Paketleme

Sürüm paketleri depoda tutulmaz; GitHub Releases üzerinden dağıtılır. Paket adı, üretim anındaki yerel saatten türetilen damgayla belirlenir:

```text
release/Titanium_beta-vYYYYMMDDHHMM/
release/Titanium_beta-vYYYYMMDDHHMM.zip
```

```powershell
# Hafif paket (FFmpeg'siz)
powershell -ExecutionPolicy Bypass -File scripts/release_uret.ps1

# FFmpeg dahil "tam" paket
powershell -ExecutionPolicy Bypass -File scripts/release_uret.ps1 -FfmpegYolu "C:\araclar\ffmpeg.exe"
```

Paketleme akışı:

1. `cargo build --release` ile çekirdeği derler.
2. `Titanium_beta-v<zaman>` dizinini oluşturur; exe, uzantı, kurulum/kaldırma betikleri, `KURULUM.cmd`, `KALDIR.cmd`, `UZANTI-YUKLE.cmd` ve `OKUBENI.txt` dosyalarını yerleştirir.
3. Zip'i `Compress-Archive` ile üretir; `-FfmpegYolu` verilmişse `-tam.zip` de üretilir.
4. Başarılı üretimden sonra `release/` altındaki **eski `Titanium*` paketlerini siler**; yalnızca güncel dizin, zip ve varsa güncel `-tam.zip` kalır.
5. Exe ve zip için SHA-256 özetlerini basar.

> Zaman damgası sürümün parçasıdır; aynı dakika içinde iki kez üretim yapılırsa aynı ad kullanılır ve mevcut hedef dosyalar baştan oluşturulur.

Son kullanıcı paketinin doğrulaması (`release/` altında güncel bir zip bulunmalıdır; yoksa önce `release_uret.ps1` çalıştırılır):

```powershell
python tests/release_simulasyon.py
```

Bu betik zip'i geçici klasöre açar, sessiz kurulum yapar, kayıt defterini/BOM'u/kodlamayı/PING'i doğrular ve sonunda HKCU kayıtlarını yedekten geri yükler.

---

## Native Messaging Protokolü

Taşıma katmanı Chrome'un standart Native Messaging çerçevesidir: her mesaj **4 bayt little-endian uzunluk + UTF-8 JSON** gövdesidir. Çekirdek gelen mesajlarda `16 MB` üst sınırını uygular; bozuk JSON sessizce yok sayılır ve kanal açık kalır. `stdin` kapanınca çekirdek `SHUTDOWN` davranışıyla sonlanır.

### Gelen komutlar (`action`)

| `action` | Alanlar | Not |
|---|---|---|
| `PING` | — | `PONG` döner |
| `START_DOWNLOAD` | `url`, `filename?`, `headers?`, `concurrency?` | `concurrency` varsayılan 16, `1..32` aralığına kısılır |
| `CANCEL` | `download_id` | İşi iptal eder; dosya/jurnal korunur |
| `PAUSE` | `download_id` | Yeni pencere/parça alımını durdurur |
| `RESUME` | `download_id` | Duraklatılmış işi sürdürür |
| `REFRESH_LINK` | `download_id`, `url`, `headers?` | Yeni oturum bilgisiyle bağlamı günceller |
| `LIST` | — | Bellekteki işlerin anlık görüntüsünü döner |
| `SHUTDOWN` | — | Çekirdek döngüsünü sonlandırır |

Örnek:

```json
{
  "action": "START_DOWNLOAD",
  "url": "https://ornek.com/video.m3u8",
  "filename": "video.mp4",
  "headers": {
    "User-Agent": "Mozilla/5.0 ...",
    "Referer": "https://ornek.com/izle",
    "Cookie": "oturum=..."
  },
  "concurrency": 16
}
```

### Giden olaylar (`status`)

| `status` | Alanlar | Anlam |
|---|---|---|
| `STARTED` | `download_id`, `url`, `filename` | İş kabul edildi; dosya adı bu aşamada boş olabilir |
| `PROGRESS` | `download_id`, `downloaded_bytes`, `total_bytes`, `speed_bps`, `eta_sec?`, `detail?` | 500 ms'de bir; hız EMA ile yumuşatılır |
| `COMPLETED` | `download_id`, `final_path` | İndirme bitti; hedef dosya yolu verilir |
| `FAILED` | `download_id`, `error` | Türkçe hata iletisi |
| `CANCELED` | `download_id` | İptal tamamlandı |
| `REFRESH_LINK_REQUIRED` | `download_id` | Sunucu oturumu reddetti; uzantı bağlantıyı yenilemeli |
| `REFRESHED` | `download_id` | Yeni bağlantı bilgisi uygulandı |
| `PONG` | — | `PING` yanıtı |
| `LIST_RESULT` | `jobs[]` | `download_id`, `url`, `filename`, `state`, `downloaded_bytes`, `total_bytes`, `speed_bps` |

`jobs[].speed_bps` anlık liste görüntüsünde `0.0`'dır; gerçek hız `PROGRESS` olayındaki `speed_bps` alanından gelir.

**İş kimliği:** `BLAKE3(url + "\0" + filename)` özetinin ilk 16 onaltılık karakteri. Aynı URL ve dosya adıyla, hâlihazırda `indiriliyor` veya `duraklatildi` durumunda bir iş varsa yeni iş açılmaz; aynı kimlik döner.

---

## Çekirdek Motoru

### 1. Keşif (`probe.rs`)

- Önce `HEAD` isteği gönderilir. `200` ve benzeri başarılı yanıtta `Content-Length`, `Accept-Ranges`, `Content-Type`, `Content-Disposition` ve yönlendirme sonrası URL alınır.
- Sunucu `HEAD` için `400/405/501` dönerse tek baytlık `GET Range: bytes=0-0` ile keşif yapılır. `206` yanıtında toplam boyut `Content-Range`'den okunur.
- Tür sınıflandırması: `mpegurl` / `.m3u8` → **HLS**, `dash+xml` / `.mpd` → **DASH**, diğerleri → **HTTP**.
- Dosya adı sırası: istekteki `filename` → `Content-Disposition` (`filename*` UTF-8 dahil) → URL'nin son parçası → `indirme_<unix>`. Ad; `<>:"/\|?*`, kontrol karakterleri ve Windows ayrılmış adlarına (`CON`, `NUL`, `COM1`…) karşı temizlenir, 180 karakterle sınırlanır.

### 2. HTTP paralel indirme (`engine.rs`)

- Range destekleniyorsa ve toplam boyut biliniyorsa paralel motor çalışır. Aksi halde tek akış kullanılır.
- Varsayılan eşlik 16; `1..32` aralığına kısılır.
- **Pencere boyutu:** 1 MB (`WINDOW_BYTES`). Her işçi kesitini 1 MB'lık pencerelerle indirir.
- **İş paylaşımı:** Bekleyen kesitler en uzundan kısa doğru sıralanır. En uzun kesit 8 MB'tan (`STEAL_THRESHOLD = 2 × 4 MB`) büyükse yarıdan bölünür. Bekleyen iş kalmazsa, en çok baytı kalan aktif kesitin ortasından iş çalınır (CAS ile atomik bitiş güncellemesi).
- **Yazma:** Her parça, hedef dosyadaki mutlak ofsete `seek_write` (Windows) veya `write_all_at` (Unix) ile yazılır. Parça tamamlandığında `sync_data` çağrılır ve jurnal güncellenir.
- **Yeniden deneme:** En fazla 10 deneme; üstel geri çekilme `400 ms × 2^n`, üst sınır 8 sn.
- **Zaman aşımları:** Bağlantı 15 sn, okuma 30 sn, akış durgunluğu 60 sn, boşta bağlantı havuzu 90 sn.
- **Durum kodları:** `401/403/410` → `REFRESH_LINK_REQUIRED` ve en fazla 600 sn bekleme; `429` ve `5xx` → geri çekilip yeniden dene; diğer başarısız kodlar → `FAILED`. Sıfırdan büyük ofsette `200` dönerse sunucu Range'i yok saymıştır ve iş açık hatayla durur.
- **HTTP istemcisi:** `tcp_nodelay`, `connect_timeout`, `read_timeout`, `pool_idle_timeout`, `pool_max_idle_per_host` ayarlarıyla kurulur.

### 3. Tek akış

Boyut bilinmiyorsa veya `Accept-Ranges: bytes` yoksa tek bağlantıyla akış yazılır. Bu modda paralel kesitleme ve jurnal tabanlı devam ettirme yoktur. Bağlantı koparsa aynı konumdan `Range` ile yeniden denenir.

### 4. Dosya ve jurnal (`storage.rs`)

- Windows'ta `DeviceIoControl(FSCTL_SET_SPARSE)` ile dosya seyrek işaretlenir; `set_len(toplam)` fiziksel sıfır yazmaz. Unix'te `fallocate` kullanılır; başarısız olursa `set_len` ile geri düşülür.
- Jurnal yolu: `<hedef>.titan_state`.
- Başlık: `u64 magic` (`0x544954414E535401`), `u64 toplam`, `u32 kayıt sayısı`; ardından `(u64 başlangıç, u64 bitiş)` kayıtları.
- Her tamamlanan kesit sonunda jurnal `sync_all` ile diske işlenir.
- Yeniden başlatmada toplam boyut eşleşiyorsa tamamlanan aralıklar birleştirilir, tümleyen aralıklar indirilir.
- Başarılı indirmede jurnal silinir. İptal veya hata durumunda dosya ve jurnal yerinde kalır.

---

## HLS ve DASH

### HLS (`hls.rs`)

1. Oynatma listesi indirilir ve `m3u8-rs` ile ayrıştırılır.
2. Ana liste ise en yüksek `(bandwidth, çözünürlük alanı)` varyantı seçilir ve alt liste indirilir.
3. `EXT-X-ENDLIST` yoksa akış canlı kabul edilir ve reddedilir.
4. `EXT-X-MAP` varsa başlangıç bloğu ilk parça olarak indirilir. Byte-range (`EXT-X-BYTERANGE`) desteklenir.
5. Anahtarlar:
   - `METHOD=NONE` → şifresiz.
   - `METHOD=AES-128` → anahtar URI'sinden 16 bayt indirilir; IV varsa onaltılıktan, yoksa medya sıra numarasından büyük-endian üretilir; CBC/PKCS7 ile çözülür.
   - `SAMPLE-AES` veya `identity` dışı `KEYFORMAT` → DRM hatası.
6. Parçalar 8 eşzamanlı akışla (`HLS_CONCURRENCY`) indirilir; sıra korunarak yazılır.
7. FFmpeg varsa ham TS/fMP4 verisi `ffmpeg -i pipe:0 -c copy -movflags +faststart` ile `.mp4` çıktısına aktarılır. FFmpeg yoksa TS akışı `.ts`, fMP4 akışı `.mp4` olarak yazılır.

### DASH (`dash.rs`)

1. MPD, `quick-xml` ile ayrıştırılır; `BaseURL`, `Period`, `AdaptationSet`, `Representation`, `SegmentTemplate`, `SegmentList`, `SegmentBase` ve `ContentProtection` işlenir.
2. `MPD@type="dynamic"` → canlı akış reddedilir.
3. `ContentProtection` içinde `cenc` veya `default_KID` → DRM reddedilir.
4. Video seti `contentType`/`mimeType` ile bulunur; en yüksek `(genişlik × yükseklik, bandwidth)` temsili seçilir. Ses setinde en yüksek `bandwidth` temsili seçilir.
5. Segment kaynakları:
   - **SegmentList:** `Initialization@sourceURL`/`range` ve `SegmentURL@media`/`mediaRange`.
   - **SegmentTemplate + SegmentTimeline:** `S@t`, `S@d`, `S@r` ile parça URL'leri üretilir; `$Number$`, `$Time$`, `$RepresentationID$`, `$Bandwidth$` ve `%0Nd` biçimleri desteklenir.
   - **SegmentTemplate + duration:** Parça sayısı, MPD toplam süresi ve `@duration/@timescale` ile hesaplanır. Toplam süre yoksa iş reddedilir.
   - **SegmentBase:** Temsil tek dosya kabul edilir ve paralel motorla indirilir.
6. Video `.video.mp4`, ses `.audio.m4a` olarak yazılır; FFmpeg varsa tek `.mp4` içinde birleştirilir ve ara dosyalar silinir.

---

## Dayanıklılık ve Devam Ettirme

- **Jurnal tabanlı devam:** Yalnızca toplam boyutu bilinen ve Range destekleyen HTTP indirmelerinde geçerlidir. HLS/DASH indirmeleri jurnal kullanmaz.
- **Süreç çökmesi:** Son tamamlanan kesit jurnale işlendiği için süreç öldürülse bile aynı URL + dosya adıyla başlatılan iş, tamamlanan aralıkları atlayarak devam eder.
- **Bağlantı yenileme:** `401/403/410` alındığında çekirdek `REFRESH_LINK_REQUIRED` yayınlar. Uzantı, kayıt altındaki kaynak URL'yi `credentials: "include"` ile tekrar çeker, yönlendirme sonrası adresi ve güncel çerezleri toplar, `REFRESH_LINK` gönderir. 600 saniye içinde yenileme gelmezse iş hata ile durur.
- **Duraklatma:** `PAUSE` yeni pencere/parça alımını durdurur; açık HTTP isteği tamamlanana kadar kısa bir gecikme olabilir.
- **İptal:** `CANCEL` işçileri durdurur; kısmi dosya ve `.titan_state` silinmez. Aynı adla yeniden başlatma devam eder.
- **Bellek durumu:** İş listesi çekirdek sürecinin belleğindedir. Çekirdek yeniden başlarsa `LIST` boş döner; uzantı deposundaki geçmiş kayıtlar korunur.

---

## Güvenlik ve Gizlilik

- **Yerel çalışma:** Tüm indirme trafiği ve veri işleme yerel çekirdekte gerçekleşir. Telemetri, uzak günlük veya bulut aktarımı yoktur.
- **Native host izni:** Host bildirimi yalnızca `chrome-extension://hhpiommjmnandklbedpjhhclpfmlnlih/` kaynağına izin verir. Kayıt `HKCU` altındadır; yönetici yetkisi gerekmez.
- **Oturum bilgisi:** Uzantı `chrome.cookies.getAll({url})` ile yalnızca hedef URL'nin çerezlerini toplar; `User-Agent`, `Referer`, `Origin` ve `Accept` başlıklarını yerel çekirdeğe iletir. Bu bilgiler yalnızca indirme isteğinde kullanılır.
- **Girdi doğrulama:** Yalnızca `http(s)` adresleri kabul edilir. Dosya adları Windows'a uygun biçimde temizlenir; ayrılmış adlar `_` ile önlenir.
- **Hata gösterimi:** Kullanıcıya Türkçe, eylem öneren hata iletileri gösterilir; yığın izleri arayüze taşınmaz.
- **DRM sınırı:** SAMPLE-AES, CENC ve `identity` dışı HLS anahtar biçimleri tespit edilip reddedilir; koruma atlatılmaz.
- **Anahtar dosyası:** `host/titan_key.pem` uzantı kimliğini sabitleyen yerel özel anahtardır; depoda tutulmaz, paylaşılmamalıdır.
- **İzinler:** Uzantı `nativeMessaging`, `webRequest`, `cookies`, `storage`, `contextMenus`, `alarms`, `notifications` izinlerini ve `<all_urls>` host iznini kullanır. `webRequest` yalnızca pasif başlık gözlemi için, bloklamadan çalışır.

---

## Bilinçli Sınırlar

| Sınır | Davranış |
|---|---|
| Canlı HLS (`EXT-X-ENDLIST` yok) | Açık hatayla reddedilir |
| Dinamik DASH (`MPD@type="dynamic"`) | Açık hatayla reddedilir |
| DRM (HLS SAMPLE-AES, DASH CENC/CENC `default_KID`) | Açık hatayla reddedilir |
| MSE `blob:` akışları | Yalnızca metaveri yakalanır; blob baytları indirilemez |
| HLS ayrı ses rendition'ları (`EXT-X-MEDIA`) | Özel işleme yoktur; seçilen varyantın segmentleri indirilir |
| Çok dönemli DASH | Özel işleme yoktur; ilk video/ses uyum setleri kullanılır |
| `SegmentTemplate@duration` + toplam süre yok | İş reddedilir |
| `SegmentTimeline` açık uçlu tekrar (`S@r<0`) | Açık hatayla reddedilir |
| Range'i yok sayan sunucuda devam | Açık hatayla durur; baştan indirme gerekir |
| `REFRESH_LINK` (HLS/DASH) | Yalnızca başlıklar güncellenir; manifest/segment adresleri yenilenmez |
| Devam jurnalı eşleştirme | Jurnal yalnızca toplam boyutla eşleşir; aynı ad ve boyutta farklı içerik için dosyayı yeniden adlandırın |
| HLS/DASH aynı adla ikinci indirme | Mevcut çıktı dosyasının üzerine yazar |
| Manuel kalite seçimi | Yoktur; en yüksek kalite otomatik seçilir |
| Windows dışı kurulum | Çekirdekte Unix dosya G/Ç dalları vardır; kurulum/paketleme Windows içindir |
| Eşzamanlı iş sınırı | Ayrı bir kuyruk sınırı yoktur; her `START_DOWNLOAD` kendi görevini açar |

---

## Sorun Giderme

| Belirti | Neden | Çözüm |
|---|---|---|
| "Çekirdek bağlı değil" | Host kaydı yok, exe taşınmış veya uzantı kimliği değişmiş | `KURULUM.cmd` / `install.ps1` çalıştırın; `HKCU\Software\Google\Chrome\NativeMessagingHosts\com.titan.downloader` değerini kontrol edin; uzantıyı yeniden yükleyin |
| "Manifest dosyası eksik veya okunamıyor" | Zip kökü veya yanlış klasör seçilmiş | `%LOCALAPPDATA%\TitanDownloader\extension` klasörünü seçin |
| 401/403/410 ve ardından hata | Oturum çerezi süresi dolmuş; kaynak sayfa etkileşim istiyor | Sayfayı yeniden açıp indirmeyi tekrar başlatın; 600 sn içinde yenileme gelmezse iş durur |
| "Sunucu Range başlığını yoksaydı" | Sunucu kısmi içerik yerine tam gövde döndürüyor | Sunucu Range desteklemiyorsa baştan indirme gerekir |
| "DRM korumalı akışlar indirilemez" | SAMPLE-AES/CENC tespit edildi | Desteklenmez; koruma atlatılmaz |
| "Canlı yayın akışı" / "Dinamik DASH" | VOD olmayan manifest | Kayıt tamamlandıktan sonra tekrar deneyin |
| HLS/DASH çıktısı ayrı dosyalar | FFmpeg bulunamadı | `ffmpeg.exe` dosyasını `%LOCALAPPDATA%\TitanDownloader\` yanına koyun veya PATH'e ekleyin |
| Testlerde ffprobe hatası | FFmpeg/ffprobe kurulu değil | FFmpeg'i PATH'e ekleyin veya tam paketi kullanın |
| PowerShell betiği engellendi | Execution policy | `powershell -ExecutionPolicy Bypass -File ...` kullanın (`KURULUM.cmd` bunu yapar) |
| Uzantı kimliği beklenenden farklı | `manifest.json` anahtarı değişmiş | `make_key.py` mevcut anahtarı korur; anahtar değiştiyse kurulumu yeniden çalıştırın |

---

## Test ve Doğrulama

### Otomatik testler

`tests/fixture/` depoda tutulmaz; testlerden önce `python tests/fixture_uret.py` ile üretilir (FFmpeg gerektirir).

| Komut | Kapsam | Beklenen sonuç |
|---|---|---|
| `cargo test --manifest-path core/Cargo.toml` | DASH MPD ayrıştırma birim testi | 1 test geçer |
| `python tests/test_nm.py` | 8 uçtan uca NM testi | `8/8 test geçti` |
| `python tests/release_simulasyon.py` | Zip → sessiz kurulum → kayıt/BOM/PING | `SON KULLANICI SİMÜLASYONU: TÜM KONTROLLER GEÇTİ` |

`test_nm.py` kapsamı:

1. `PING/PONG`
2. 16 MB çok bağlantılı Range indirmesi + kaynakla bayt bayt SHA-256 eşleşmesi + jurnal temizliği
3. HLS düz (fMP4) → MP4; ffprobe ile video+ses doğrulaması
4. HLS AES-128 → MP4; çözme ve ffprobe doğrulaması
5. DASH video+ses → tek MP4; ffprobe doğrulaması
6. İptal akışı (`CANCELED`)
7. `LIST` / `LIST_RESULT`
8. Süreç zorla öldürülüp `.titan_state` üzerinden devam ettirme; SHA-256 doğrulaması

Testler `tests/test_server.py` üzerinden `127.0.0.1:8471` adresinde çalışır; yavaş senaryolar için sunucu kontrollü gecikme uygular. İndirme testleri `%USERPROFILE%\Downloads` altına yazar ve sonunda temizler.

### Kanıt standardı

Bir değişiklik "tamamlandı" sayılmadan önce:

- `cargo build --release` ve `cargo test` geçmeli,
- ilgili uçtan uca test (NM, HLS, DASH veya WAL) geçmeli,
- paketleme değiştiyse `release_simulasyon.py` geçmeli,
- yeni bir davranış eklendiyse bu belge ve `README.md` güncellenmelidir.

---

## Belge Bakımı

Bu dosya `docs/readme.md` olarak tutulur. Kod ile belge çelişirse **kod esas alınır**; çelişki tespit edildiğinde belge aynı değişiklikte güncellenir. Sürüm paketi adlandırma kuralı `Titanium_beta-vYYYYMMDDHHMM` olup `scripts/release_uret.ps1` içinde uygulanır.
