$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

# 产物直接输出到标准前端目录 web\wasm\，不再生成 pkg\ 目录
$outDir = Join-Path $root "web\wasm"

Write-Host "==> wasm-pack build --target web -> web\wasm" -ForegroundColor Cyan
wasm-pack build (Join-Path $root "crates\js2png-wasm") `
  --target web `
  --out-dir $outDir `
  --release
if ($LASTEXITCODE -ne 0) { throw "WASM 构建失败" }

Write-Host ""
Write-Host "WASM 产物已输出到：$outDir" -ForegroundColor Green
Get-ChildItem -LiteralPath $outDir -File | Select-Object Name, Length | Format-Table -AutoSize
