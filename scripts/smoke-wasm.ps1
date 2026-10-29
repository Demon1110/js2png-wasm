$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

Write-Host "==> Node 冒烟测试（WASM 往返 + 密钥机制）" -ForegroundColor Cyan
node "$root\scripts\smoke.mjs"
if ($LASTEXITCODE -ne 0) { throw "冒烟测试失败" }
