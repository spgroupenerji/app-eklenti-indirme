# Titanium İndirme Hızlandırıcı — Kaldırma
$ErrorActionPreference = "Stop"

# Türkçe çıktı: boru ve konsol her zaman UTF-8
$OutputEncoding = [System.Text.UTF8Encoding]::new($false)
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)


foreach ($yol in @(
        "HKCU:\Software\Google\Chrome\NativeMessagingHosts\com.titan.downloader",
        "HKCU:\Software\Microsoft\Edge\NativeMessagingHosts\com.titan.downloader",
        "HKCU:\Software\BraveSoftware\Brave-Browser\NativeMessagingHosts\com.titan.downloader"
    )) {
    if (Test-Path $yol) {
        Remove-Item $yol -Recurse -Force
        Write-Host "[OK] $yol kaldırıldı" -ForegroundColor Green
    }
}

$hedef = "$env:LOCALAPPDATA\TitanDownloader"
if (Test-Path $hedef) {
    Remove-Item $hedef -Recurse -Force
    Write-Host "[OK] $hedef kaldırıldı" -ForegroundColor Green
}

Write-Host "Kaldırma tamamlandı. Uzantıyı chrome://extensions sayfasından ayrıca kaldırın." -ForegroundColor Cyan
