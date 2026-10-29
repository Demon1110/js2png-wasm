// js2png-wasm 冒烟测试（Node 环境）
// 验证：WASM 绑定可加载，编码/解密往返一致，密钥机制生效。
// 运行：node scripts/smoke.mjs

import { readFile } from 'node:fs/promises';
import { fileURLToPath, pathToFileURL } from 'node:url';
import path from 'node:path';

const here = path.dirname(fileURLToPath(import.meta.url));
const pkgDir = path.resolve(here, '..', 'web', 'pkg');

// web target 的 init 在 Node 中无法 fetch file://，直接传入 WebAssembly.Module
const mod = await import(pathToFileURL(path.join(pkgDir, 'js2png_wasm.js')).href);
const { default: init, encode_js, encode_js_with_key, decode_png, decode_png_with_key, png_dimensions } = mod;

const wasmBytes = await readFile(path.join(pkgDir, 'js2png_wasm_bg.wasm'));
await init({ module_or_path: new WebAssembly.Module(wasmBytes) });

let failed = 0;
function check(name, cond, extra = '') {
  if (cond) {
    console.log(`  [PASS] ${name}`);
  } else {
    failed++;
    console.log(`  [FAIL] ${name} ${extra}`);
  }
}

console.log('== 冒烟测试：无密钥往返 ==');
{
  const src = "console.log('wasm smoke test 中文');\nfunction add(a,b){return a+b;}\n";
  const png = encode_js(src);
  check('encode_js 返回 Uint8Array', png instanceof Uint8Array, `类型=${png.constructor?.name}`);
  const [w, h] = png_dimensions(png);
  check('PNG 尺寸有效', w > 0 && h > 0, `${w}x${h}`);
  // 独立验证 PNG 签名
  const sig = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
  check('PNG 签名合法', sig.every((v, i) => png[i] === v));
  const back = decode_png(png);
  check('decode_png 还原一致', back === src);
}

console.log('== 冒烟测试：带密钥往返 ==');
{
  const src = 'const secret = "hello";\nexport default secret;\n';
  const key = 'wasm-test-key';
  const png = encode_js_with_key(src, key);
  const back = decode_png_with_key(png, key);
  check('decode_png_with_key 还原一致', back === src);
  // 错误密钥必须抛错
  let threw = false;
  try {
    decode_png_with_key(png, 'wrong-key');
  } catch {
    threw = true;
  }
  check('错误密钥抛出异常', threw);
  // 不传密钥必须抛错
  threw = false;
  try {
    decode_png(png);
  } catch {
    threw = true;
  }
  check('加密数据不传密钥抛出异常', threw);
}

console.log('== 冒烟测试：无密钥 PNG 用带密钥接口也能解 ==');
{
  const src = 'plain data';
  const png = encode_js(src);
  const back = decode_png_with_key(png, 'any-key-ignored');
  check('key 参数对未加密数据无副作用', back === src);
}

console.log(failed === 0 ? '\n全部冒烟测试通过 ✓' : `\n${failed} 项失败 ✗`);
process.exit(failed === 0 ? 0 : 1);
