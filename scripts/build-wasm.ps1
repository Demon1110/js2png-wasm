$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

Write-Host "==> wasm-pack build（WASM 绑定，web target）" -ForegroundColor Cyan
wasm-pack build "$root\crates\js2png-wasm" --target web --out-dir "$root\web\pkg" --release
if ($LASTEXITCODE -ne 0) { throw "WASM 构建失败" }
