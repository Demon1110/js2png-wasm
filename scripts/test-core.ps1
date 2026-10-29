$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

Write-Host "==> cargo test（核心库单元测试）" -ForegroundColor Cyan
cargo test -p js2png-core
if ($LASTEXITCODE -ne 0) { throw "单元测试失败" }
