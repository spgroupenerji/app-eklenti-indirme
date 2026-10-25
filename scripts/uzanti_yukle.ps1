# Titanium — uzantıyı tarayıcıya yükleme yardımcısı
# Doğru klasörü Explorer'da açar, yolu panoya kopyalar ve Uzantılar sayfasını açar.
param(
    [string]$KurulumDizini = "$env:LOCALAPPDATA\TitanDownloader",
    [switch]$Sessiz
)

$ErrorActionPreference = "Stop"

# Türkçe çıktı: boru ve konsol her zaman UTF-8
$OutputEncoding = [System.Text.UTF8Encoding]::new($false)
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)


function Aday([string[]]$yollar) {
    $yollar | Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
}

$uzantiHedef = Join-Path $KurulumDizini "extension"
$manifestYolu = Join-Path $uzantiHedef "manifest.json"

if (-not (Test-Path $manifestYolu)) {
    Write-Host "HATA: Uzantı henüz kurulmamış." -ForegroundColor Red
    Write-Host "Önce paketteki KURULUM.cmd dosyasını çalıştırın." -ForegroundColor Yellow
    Write-Host "Beklenen klasör: $uzantiHedef" -ForegroundColor Gray
    exit 1
}

$panoDurumu = $null
if (-not $Sessiz) {
    try {
        Set-Clipboard -Value $uzantiHedef
        $panoDurumu = "Uzantı klasörü yolu panoya kopyalandı."
    } catch {
        $panoDurumu = "Uyarı: Yol panoya kopyalanamadı."
    }
}

Write-Host ""
Write-Host "UZANTIYI YÜKLEME" -ForegroundColor Cyan
Write-Host "  1. chrome://extensions sayfasında 'Geliştirici modu'nu açın."
Write-Host "  2. 'Paketlenmemiş yükle' düğmesine basın."
Write-Host "  3. Şu klasörü seçin:" -ForegroundColor Yellow
Write-Host "     $uzantiHedef" -ForegroundColor Yellow
Write-Host ""
Write-Host "ÖNEMLİ: Zip'i açtığınız Titanium_beta-v... klasörünü DEĞİL, yukarıdaki extension klasörünü seçin." -ForegroundColor Red
Write-Host "        manifest.json yalnızca extension alt klasöründedir." -ForegroundColor Red
if ($panoDurumu) { Write-Host $panoDurumu -ForegroundColor Gray }

if ($Sessiz) { exit 0 }

Start-Process explorer.exe $uzantiHedef

$chromeCalisan = $null
try {
    $chromeCalisan = Get-Process chrome -ErrorAction SilentlyContinue |
        Select-Object -First 1 -ExpandProperty Path
} catch {
    $chromeCalisan = $null
}
$chrome = Aday @(
    $chromeCalisan,
    "$env:ProgramFiles\Google\Chrome\Application\chrome.exe",
    "${env:ProgramFiles(x86)}\Google\Chrome\Application\chrome.exe",
    "$env:LOCALAPPDATA\Google\Chrome\Application\chrome.exe"
)

if ($chrome) {
    Start-Process $chrome "chrome://extensions"
    Write-Host "Chrome, Uzantılar sayfasında açılıyor..." -ForegroundColor Gray
} else {
    Write-Host "Tarayıcınızda chrome://extensions adresini açın." -ForegroundColor Gray
}
