# ============================================================
# js2png-wasm 一键构建脚本（Windows / PowerShell）
# 用法：pwsh -File scripts\build-all.ps1
# 产物：
#   1. CLI 发布二进制     -> target\release\js2png-cli.exe
#   2. WASM 包            -> web\pkg\（web target，供演示页与交付页面使用）
#   3. 单元测试 + WASM 冒烟测试
# ============================================================
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

Write-Host "==> [1/4] cargo test（核心库单元测试）" -ForegroundColor Cyan
cargo test -p js2png-core
if ($LASTEXITCODE -ne 0) { throw "单元测试失败" }

Write-Host "==> [2/4] cargo build --release（CLI 发布二进制）" -ForegroundColor Cyan
cargo build -p js2png-cli --release
if ($LASTEXITCODE -ne 0) { throw "CLI 构建失败" }

Write-Host "==> [3/4] wasm-pack build（WASM 绑定，web target）" -ForegroundColor Cyan
wasm-pack build "$root\crates\js2png-wasm" --target web --out-dir "$root\web\pkg" --release
if ($LASTEXITCODE -ne 0) { throw "WASM 构建失败" }

Write-Host "==> [4/4] Node 冒烟测试（WASM 往返 + 密钥机制）" -ForegroundColor Cyan
node "$root\scripts\smoke.mjs"
if ($LASTEXITCODE -ne 0) { throw "冒烟测试失败" }

Write-Host ""
Write-Host "全部构建与测试通过 ✓" -ForegroundColor Green
Write-Host "  CLI : $root\target\release\js2png-cli.exe"
Write-Host "  WASM: $root\web\pkg\"
Write-Host "演示页：cd $root && python -m http.server 8765，然后访问 http://localhost:8765/web/"
