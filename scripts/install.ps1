# Titanium İndirme Hızlandırıcı — Kurulum
# Hem sürüm paketinde (yanında TitanDownloader/ ve extension/) hem geliştirici
# deposunda (scripts/) çalışır. Son kullanıcı hiçbir bağımlılık kurmaz.
param(
    [string]$KurulumDizini = "$env:LOCALAPPDATA\TitanDownloader",
    [switch]$Sessiz
)

$ErrorActionPreference = "Stop"

# Türkçe çıktı: boru ve konsol her zaman UTF-8
$OutputEncoding = [System.Text.UTF8Encoding]::new($false)
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)


function Aday([string[]]$yollar) {
    $yollar | Where-Object { Test-Path $_ } | Select-Object -First 1
}

$exeKaynak = Aday @(
    (Join-Path $PSScriptRoot "TitanDownloader\titan-downloader-core.exe"),
    (Join-Path $PSScriptRoot "..\core\target\release\titan-downloader-core.exe")
)
$manifestYolu = Aday @(
    (Join-Path $PSScriptRoot "extension\manifest.json"),
    (Join-Path $PSScriptRoot "..\extension\manifest.json")
)

if (-not $exeKaynak) {
    Write-Host "HATA: titan-downloader-core.exe bulunamadı." -ForegroundColor Red
    Write-Host "Sürüm paketini açtığınız klasörden KURULUM.cmd çalıştırın." -ForegroundColor Yellow
    exit 1
}
if (-not $manifestYolu) {
    Write-Host "HATA: extension\manifest.json bulunamadı." -ForegroundColor Red
    exit 1
}
$uzantiKaynak = Split-Path -Parent $manifestYolu

Write-Host "Titanium kuruluyor..." -ForegroundColor Cyan

# 1) Çekirdek ve uzantı kararlı konuma kopyalanır
New-Item -ItemType Directory -Force -Path $KurulumDizini | Out-Null
Copy-Item $exeKaynak (Join-Path $KurulumDizini "titan-downloader-core.exe") -Force

$uzantiHedef = Join-Path $KurulumDizini "extension"
if (Test-Path $uzantiHedef) { Remove-Item $uzantiHedef -Recurse -Force }
Copy-Item $uzantiKaynak $uzantiHedef -Recurse -Force

# ffmpeg.exe pakette varsa yanına kopyalanır (isteğe bağlı kayıpsız birleştirme)
$ffmpegKaynak = Join-Path (Split-Path -Parent $exeKaynak) "ffmpeg.exe"
if (Test-Path $ffmpegKaynak) {
    Copy-Item $ffmpegKaynak (Join-Path $KurulumDizini "ffmpeg.exe") -Force
    Write-Host "[OK] ffmpeg.exe kopyalandı (kayıpsız birleştirme etkin)" -ForegroundColor Green
}

# 2) Uzantı kimliği manifest anahtarından hesaplanır (kararlı kimlik)
$manifest = Get-Content $manifestYolu -Raw | ConvertFrom-Json
$der = [Convert]::FromBase64String($manifest.key)
$sha = [System.Security.Cryptography.SHA256]::Create()
$ozet = $sha.ComputeHash($der)[0..15]
$alfabe = "abcdefghijklmnop"
$kimlik = -join ($ozet | ForEach-Object { $alfabe[$_ -shr 4] + $alfabe[$_ -band 15] })

# 3) Yerel mesajlaşma ana makinesi bildirimi (BOM'suz UTF-8 zorunlu)
$hostJson = Join-Path $KurulumDizini "com.titan.downloader.json"
$icerik = @{
    name            = "com.titan.downloader"
    description     = "Titanium Download Accelerator Native Messaging Host"
    path            = (Join-Path $KurulumDizini "titan-downloader-core.exe")
    type            = "stdio"
    allowed_origins = @("chrome-extension://$kimlik/")
} | ConvertTo-Json
[System.IO.File]::WriteAllText($hostJson, $icerik, [System.Text.UTF8Encoding]::new($false))

# 4) Kayıt defteri (Chrome, Edge, Brave)
foreach ($tarayici in @(
        @{ Ad = "Chrome"; Yol = "Software\Google\Chrome\NativeMessagingHosts" },
        @{ Ad = "Edge";   Yol = "Software\Microsoft\Edge\NativeMessagingHosts" },
        @{ Ad = "Brave";  Yol = "Software\BraveSoftware\Brave-Browser\NativeMessagingHosts" }
    )) {
    $anahtar = "HKCU:\$($tarayici.Yol)\com.titan.downloader"
    New-Item -Path $anahtar -Force | Out-Null
    Set-ItemProperty -Path $anahtar -Name "(default)" -Value $hostJson
    Write-Host "[OK] $($tarayici.Ad) kaydı yazıldı" -ForegroundColor Green
}

Write-Host ""
Write-Host "Kurulum tamamlandı." -ForegroundColor Cyan
Write-Host "  Uzantı kimliği : $kimlik"
Write-Host ""
Write-Host "SON ADIM (tek seferlik):" -ForegroundColor Yellow
Write-Host "  1. Açılan Uzantılar sayfasında 'Geliştirici modu'nu açın."
Write-Host "  2. 'Paketlenmemiş yükle' düğmesine basın."
Write-Host "  3. Şu klasörü seçin: $uzantiHedef"
Write-Host ""
Write-Host "ÖNEMLİ: Zip'i açtığınız Titanium_beta-v... klasörünü DEĞİL, yukarıdaki extension klasörünü seçin." -ForegroundColor Red
Write-Host "        manifest.json yalnızca extension alt klasöründedir." -ForegroundColor Red

if (-not $Sessiz) {
    try {
        Set-Clipboard -Value $uzantiHedef
        Write-Host "[OK] Uzantı klasörü yolu panoya kopyalandı." -ForegroundColor Green
    } catch {
        Write-Host "Uyarı: Uzantı klasörü yolu panoya kopyalanamadı." -ForegroundColor Yellow
    }
}

if (-not $Sessiz) {
    Start-Process explorer.exe $uzantiHedef
    Write-Host "Uzantı klasörü Explorer'da açıldı." -ForegroundColor Gray
}

$chromeCalisan = $null
try {
    $chromeCalisan = Get-Process chrome -ErrorAction SilentlyContinue |
        Select-Object -First 1 -ExpandProperty Path
} catch {
    $chromeCalisan = $null
}
$chromeAdaylar = @(
    $chromeCalisan,
    "$env:ProgramFiles\Google\Chrome\Application\chrome.exe",
    "${env:ProgramFiles(x86)}\Google\Chrome\Application\chrome.exe",
    "$env:LOCALAPPDATA\Google\Chrome\Application\chrome.exe"
) | Where-Object { $_ }
$chrome = Aday $chromeAdaylar
if ($Sessiz) {
    Write-Host "Sessiz kip: tarayıcı ve Explorer açılmadı." -ForegroundColor Gray
} elseif ($chrome) {
    Start-Process $chrome "chrome://extensions"
    Write-Host "Chrome, Uzantılar sayfasında açılıyor..." -ForegroundColor Gray
} else {
    Write-Host "Tarayıcınızda chrome://extensions adresini açın." -ForegroundColor Gray
}
