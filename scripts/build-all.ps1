# ============================================================
# js2png-wasm 一键构建脚本（Windows / PowerShell）
# 用法：pwsh -File scripts\build-all.ps1
# ============================================================
$ErrorActionPreference = "Stop"
$scripts = @(
    "test-core.ps1",
    "build-cli.ps1",
    "build-wasm.ps1",
    "smoke-wasm.ps1"
)

for ($i = 0; $i -lt $scripts.Count; $i++) {
    Write-Host "==> [$($i + 1)/4] $($scripts[$i])" -ForegroundColor Cyan
    & (Join-Path $PSScriptRoot $scripts[$i])
}

$root = Split-Path -Parent $PSScriptRoot
Write-Host ""
Write-Host "全部构建与测试通过 ✓" -ForegroundColor Green
Write-Host "  CLI : $root\target\release\js2png-cli.exe"
Write-Host "  WASM: $root\web\pkg\"
Write-Host "演示页：cd $root && python -m http.server 8765，然后访问 http://localhost:8765/web/"
