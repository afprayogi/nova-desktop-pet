# Membangun NOVA-Setup-<versi>.exe secara lokal.  Pakai:  powershell -ExecutionPolicy Bypass -File installer\bangun.ps1
# Langkah: cargo build --release -> Inno Setup -> dist\NOVA-Setup-<versi>.exe (+ latest.json bertanda tangan bila kunci tersedia).
$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..")

$versi = (Select-String -Path Cargo.toml -Pattern '^version\s*=\s*"(.+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
Write-Host "Versi: $versi"

cargo build --release
if ($LASTEXITCODE -ne 0) { throw "cargo build gagal" }

$kandidat = @(
  (Get-Command ISCC.exe -ErrorAction SilentlyContinue).Source,
  "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe",
  "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
  "$env:ProgramFiles\Inno Setup 6\ISCC.exe"
) | Where-Object { $_ -and (Test-Path $_) }
if (-not $kandidat) { throw "Inno Setup 6 tidak ditemukan. Pasang: winget install JRSoftware.InnoSetup" }

& @($kandidat)[0] "/DAppVersion=$versi" installer\nova.iss
if ($LASTEXITCODE -ne 0) { throw "Inno Setup gagal" }
$setup = "dist\NOVA-Setup-$versi.exe"
Write-Host "Pemasang: $setup"

# Manifest pembaruan bertanda tangan (opsional lokal; di GitHub dibuat otomatis oleh workflow rilis).
$kunciFile = Join-Path $env:USERPROFILE ".nova-rilis\kunci-privat.b64"
if (-not $env:NOVA_SIGNING_KEY -and (Test-Path $kunciFile)) { $env:NOVA_SIGNING_KEY = (Get-Content $kunciFile -Raw).Trim() }
if ($env:NOVA_SIGNING_KEY) {
  $url = "https://github.com/afprayogi/nova-desktop-pet/releases/download/v$versi/NOVA-Setup-$versi.exe"
  $p = Start-Process -FilePath .\target\release\nova.exe -ArgumentList @("--rilis", $setup, $url, "CHANGELOG.md") -Wait -PassThru -NoNewWindow
  if ($p.ExitCode -ne 0 -or -not (Test-Path dist\latest.json)) { throw "pembuatan latest.json gagal" }
  Write-Host "Manifest: dist\latest.json"
} else {
  Write-Host "Kunci penandatangan tidak ada; latest.json dilewati."
}
