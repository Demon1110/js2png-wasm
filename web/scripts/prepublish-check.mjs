// 发布前守卫：确保 WASM 产物存在且完整，避免发出"没有解密引擎"的坏包
import { existsSync, statSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const pkgDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const required = [
  'wasm/js2png_wasm.js',
  'wasm/js2png_wasm_bg.wasm',
  'wasm/js2png_wasm.d.ts',
  'js/runtime-sdk.js',
  'js/runtime-sdk.d.ts',
  'package.json',
  'README.md',
];

const missing = required.filter((rel) => {
  const abs = path.join(pkgDir, rel);
  return !existsSync(abs) || statSync(abs).size === 0;
});

if (missing.length > 0) {
  console.error('[prepublish] 缺少必要文件：');
  for (const m of missing) console.error('  - ' + m);
  console.error('请先在仓库根目录运行 scripts/build-wasm.ps1 生成 WASM 产物。');
  process.exit(1);
}

console.log('[prepublish] 包内容自检通过（' + required.length + ' 项必需文件齐全）');
