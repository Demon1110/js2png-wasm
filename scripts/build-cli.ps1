$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

Write-Host "==> cargo build --release（CLI 发布二进制）" -ForegroundColor Cyan
cargo build -p js2png-cli --release
if ($LASTEXITCODE -ne 0) { throw "CLI 构建失败" }
