$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '../..')
& "$PSScriptRoot/icon.ps1"
if (-not (Get-Command cargo-packager -ErrorAction SilentlyContinue)) {
    cargo install cargo-packager --version 0.11.8 --locked
    if ($LASTEXITCODE) { throw 'cargo-packager installation failed' }
}
cargo build --release --locked
if ($LASTEXITCODE) { throw 'Release build failed' }
$version = (cargo metadata --no-deps --format-version 1 | ConvertFrom-Json).packages | Where-Object name -eq 'trackpad-gestures' | Select-Object -ExpandProperty version
$config = Get-Content app/packager.json -Raw | ConvertFrom-Json
$config.version = $version
$config | ConvertTo-Json -Depth 10 | Set-Content app/packager.generated.json
cargo packager --config app/packager.generated.json
if ($LASTEXITCODE) { throw 'Packaging failed' }
