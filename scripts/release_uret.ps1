# Titanium sürüm paketi üretir: derleme + paketleme + zip + bütünlük özetleri
# Kullanım:
#   powershell -ExecutionPolicy Bypass -File scripts/release_uret.ps1
#   powershell -ExecutionPolicy Bypass -File scripts/release_uret.ps1 -FfmpegYolu "C:\...\ffmpeg.exe"
param(
    [string]$FfmpegYolu = ""
)

$ErrorActionPreference = "Stop"

# Türkçe çıktı: boru ve konsol her zaman UTF-8
$OutputEncoding = [System.Text.UTF8Encoding]::new($false)
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)

$kok = Split-Path -Parent $PSScriptRoot
$zamanDamgasi = Get-Date -Format "yyyyMMddHHmm"
$betaEtiketi = "_beta-v$zamanDamgasi"
$paketAdi = "Titanium$betaEtiketi"
$hedef = Join-Path $kok "release\$paketAdi"
$zip = Join-Path $kok "release\$paketAdi.zip"
$tamZip = $null

Write-Host "[1/6] Çekirdek derleniyor..." -ForegroundColor Cyan
Push-Location (Join-Path $kok "core")
try {
    cargo build --release
    if ($LASTEXITCODE -ne 0) { throw "cargo build başarısız" }
} finally {
    Pop-Location
}

Write-Host "[2/6] Paket dizini hazırlanıyor..." -ForegroundColor Cyan
if (Test-Path $hedef) { Remove-Item $hedef -Recurse -Force }
New-Item -ItemType Directory -Force -Path (Join-Path $hedef "TitanDownloader") | Out-Null

$exe = Join-Path $kok "core\target\release\titan-downloader-core.exe"
if (-not (Test-Path $exe)) { throw "Derlenmiş exe bulunamadı" }
Copy-Item $exe (Join-Path $hedef "TitanDownloader\titan-downloader-core.exe")

Copy-Item (Join-Path $kok "extension") (Join-Path $hedef "extension") -Recurse
Copy-Item (Join-Path $PSScriptRoot "install.ps1") $hedef
Copy-Item (Join-Path $PSScriptRoot "uninstall.ps1") $hedef
Copy-Item (Join-Path $PSScriptRoot "uzanti_yukle.ps1") $hedef

Write-Host "[3/6] Çift tıklamalı başlatıcılar yazılıyor..." -ForegroundColor Cyan
$bomsuzUtf8 = [System.Text.UTF8Encoding]::new($false)
$bomluUtf8 = [System.Text.UTF8Encoding]::new($true)

$installCmd = "@echo off`r`nsetlocal`r`npowershell -NoProfile -ExecutionPolicy Bypass -File `"%~dp0install.ps1`"`r`necho.`r`npause`r`n"
[System.IO.File]::WriteAllText((Join-Path $hedef "KURULUM.cmd"), $installCmd, $bomsuzUtf8)

$kaldirCmd = "@echo off`r`nsetlocal`r`npowershell -NoProfile -ExecutionPolicy Bypass -File `"%~dp0uninstall.ps1`"`r`necho.`r`npause`r`n"
[System.IO.File]::WriteAllText((Join-Path $hedef "KALDIR.cmd"), $kaldirCmd, $bomsuzUtf8)

$yukleCmd = "@echo off`r`nsetlocal`r`npowershell -NoProfile -ExecutionPolicy Bypass -File `"%~dp0uzanti_yukle.ps1`"`r`necho.`r`npause`r`n"
[System.IO.File]::WriteAllText((Join-Path $hedef "UZANTI-YUKLE.cmd"), $yukleCmd, $bomsuzUtf8)

$okubeni = @"
TİTANİUM İNDİRME HIZLANDIRICI beta-v$zamanDamgasi
====================================

Bu paket son kullanıcılar içindir; Rust, Python veya FFmpeg kurmanız GEREKMEZ.
Çekirdek derlenmiş olarak gelir.

KURULUM (tek seferlik, 2 adım)
------------------------------
1) KURULUM.cmd dosyasına çift tıklayın.
   - Çekirdek ve uzantı %LOCALAPPDATA%\TitanDownloader konumuna kopyalanır
   - Chrome / Edge / Brave için yerel mesajlaşma kaydı yazılır
   - Chrome, Uzantılar sayfasında açılır

2) Açılan chrome://extensions sayfasında:
   a. Sağ üstten "Geliştirici modu"nu açın
   b. "Paketlenmemiş yükle" düğmesine basın
   c. Şu klasörü seçin: %LOCALAPPDATA%\TitanDownloader\extension

ÖNEMLİ: Zip'i açtığınız $paketAdi klasörünü SEÇMEYİN.
manifest.json yalnızca extension alt klasöründedir.

KOLAY YÜKLEME
-------------
KURULUM.cmd bittikten sonra UZANTI-YUKLE.cmd dosyasına çift tıklayın:
- Doğru uzantı klasörünü Explorer'da açar
- Klasör yolunu panoya kopyalar
- chrome://extensions sayfasını açar

Bitti. Video/ses içeren sayfalarda sağ üstte "Medyayı İndir" düğmesi çıkar;
bağlantı/video üzerinde sağ tık → "Titanium ile indir" de kullanılabilir.

FFMPEG (isteğe bağlıdır, zorunlu değildir)
------------------------------------------
ffmpeg.exe pakette varsa HLS (TS) ve DASH akışları kayıpsız tek MP4'e birleştirilir.
Yoksa: HLS fMP4 zaten MP4 olarak iner, TS akışlar .ts, DASH kanalları ayrı
dosyalar olarak iner (VLC/mpv gibi oynatıcılarla oynatılabilir).
Sonradan eklemek için ffmpeg.exe dosyasını şu klasöre koyun:
   %LOCALAPPDATA%\TitanDownloader\

KALDIRMA
--------
KALDIR.cmd dosyasına çift tıklayın; ardından uzantıyı chrome://extensions
sayfasından kaldırın.

SIK HATA
--------
"Manifest dosyası eksik veya okunamıyor / Manifest yüklenemedi"
  Nedeni: Zip'in kök klasörü seçilmiş.
  Doğru klasör: %LOCALAPPDATA%\TitanDownloader\extension
  Kolay yol: UZANTI-YUKLE.cmd dosyasını çalıştırın.

NOTLAR
------
- Uzantı kimliği kararlıdır (manifest anahtarı gömülü), güncellemede değişmez
- İndirmeler varsayılan olarak %USERPROFILE%\Downloads klasörüne yazılır
- Canlı yayınlar ve DRM korumalı akışlar desteklenmez; net hata iletisi gösterilir

Bilgi: README.md
"@
[System.IO.File]::WriteAllText((Join-Path $hedef "OKUBENI.txt"), ($okubeni -replace "`r?`n", "`r`n"), $bomluUtf8)

Write-Host "[4/6] Zip paketi oluşturuluyor..." -ForegroundColor Cyan
if (Test-Path $zip) { Remove-Item $zip -Force }
Compress-Archive -Path (Join-Path $hedef "*") -DestinationPath $zip -CompressionLevel Optimal

if ($FfmpegYolu -and (Test-Path $FfmpegYolu)) {
    Write-Host "[EK] Tam paket: ffmpeg.exe ekleniyor..." -ForegroundColor Cyan
    Copy-Item $FfmpegYolu (Join-Path $hedef "TitanDownloader\ffmpeg.exe") -Force
    $tamZip = Join-Path $kok "release\$paketAdi-tam.zip"
    if (Test-Path $tamZip) { Remove-Item $tamZip -Force }
    Compress-Archive -Path (Join-Path $hedef "*") -DestinationPath $tamZip -CompressionLevel Optimal
    Write-Host ("  Tam paket: {0} ({1:N1} MB)" -f $tamZip, ((Get-Item $tamZip).Length / 1MB)) -ForegroundColor Green
}

Write-Host "[5/6] Eski sürüm paketleri siliniyor..." -ForegroundColor Cyan
$korunacak = @($hedef, $zip)
if ($tamZip) { $korunacak += $tamZip }
Get-ChildItem (Join-Path $kok "release") -Force | Where-Object {
    $_.Name -like "Titanium*" -and $korunacak -notcontains $_.FullName
} | ForEach-Object {
    try {
        Remove-Item $_.FullName -Recurse -Force
        Write-Host "  Silindi: $($_.Name)"
    } catch {
        Write-Warning "  Silinemedi: $($_.Name) — $($_.Exception.Message)"
    }
}

Write-Host "[6/6] Bütünlük özetleri..." -ForegroundColor Cyan
$exeHash = (Get-FileHash (Join-Path $hedef "TitanDownloader\titan-downloader-core.exe") -Algorithm SHA256).Hash
$zipHash = (Get-FileHash $zip -Algorithm SHA256).Hash

Write-Host ""
Write-Host "Paket hazır:" -ForegroundColor Green
Write-Host "  Dizin : $hedef"
Write-Host "  Zip   : $zip ($('{0:N1}' -f ((Get-Item $zip).Length / 1MB)) MB)"
Write-Host "  Exe SHA-256 : $exeHash"
Write-Host "  Zip SHA-256 : $zipHash"
