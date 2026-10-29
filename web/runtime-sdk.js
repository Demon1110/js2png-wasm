// ============================================================
// js2png-runtime-sdk —— 交付给客户的浏览器端运行时（ESM）
//
// 客户只需三步：
//   1) 复制本目录到客户项目（pkg/ 为 WASM 产物，必须一同拷贝）
//   2) import { Js2Png, preload } from './js2png/runtime-sdk.js'
//   3) const biz = await Js2Png.load('./app.png', '密钥'); biz.call('greet', '客户')
//
// 时序保障（核心能力）：
//   - load() 是幂等的：同一 (pngUrl, key, mode) 只加载/解密一次，之后返回同一 Promise/实例
//   - preload() 可在页面启动时立即预热；业务入口用 await 门闩等待，
//     保证「用户交互发生时解密必然已完成」——极端情况下同一 load() 现场兜底
//   - 解密结果常驻内存，调用零等待
//
// 业务代码两种编写约定（二选一）：
//   A. ES Module（推荐）：export function greet(){} / export default {...}
//      -> Js2Png.load(url, key)（mode 默认 'module'），biz.call('greet', ...) 或 biz.exports.greet(...)
//   B. 普通脚本（IIFE）：window.MyApp = { greet(){} }
//      -> Js2Png.load(url, key, { mode: 'script' })，biz.call('MyApp.greet') 或 biz.exports.MyApp.greet(...)
// ============================================================

import init, { decode_png, decode_png_with_key } from './pkg/js2png_wasm.js';

let wasmReady = null;
/** 幂等初始化 WASM（wasm-bindgen 的 init 可重复调用，缓存 Promise 避免重复加载） */
function ensureWasm() {
  wasmReady ??= init();
  return wasmReady;
}

/** 实例缓存：相同输入只解密一次 */
const cache = new Map();

export class Js2Png {
  /**
   * @param {object} api        module 模式：ES 模块命名空间；script 模式：window
   * @param {string} source     解密出的原始 JS 源码
   */
  constructor(api, source) {
    this._api = api;
    this.source = source;
  }

  /** 解密模块的全部导出（module 模式）或 window（script 模式） */
  get exports() {
    return this._api;
  }

  /**
   * 调用解密模块中的函数。
   * 支持点路径：call('greet')、call('MyApp.greet')、call('default.greet')
   */
  call(path, ...args) {
    const fn = path
      .split('.')
      .reduce((o, k) => (o == null ? undefined : o[k]), this._api);
    if (typeof fn !== 'function') {
      throw new Error(`解密模块中不存在可调用的导出：${path}`);
    }
    return fn(...args);
  }
}

/**
 * 加载并解密 PNG，返回 Js2Png 实例（幂等，可安全重复调用）。
 *
 * @param {string}  pngUrl      PNG 地址（同源或 CORS 允许；建议与 WASM 同源部署）
 * @param {string}  [key=null]  加密密钥；未加密的 PNG 传 null 或 ''
 * @param {object}  [opts]
 * @param {'module'|'script'} [opts.mode='module'] 业务代码形态
 */
export async function load(pngUrl, key = null, opts = {}) {
  const mode = opts.mode || 'module';
  const cacheKey = `${pngUrl}\u0000${key ?? ''}\u0000${mode}`;
  const hit = cache.get(cacheKey);
  if (hit) return hit;

  await ensureWasm();

  const res = await fetch(pngUrl);
  if (!res.ok) {
    throw new Error(`PNG 加载失败：HTTP ${res.status} ${res.statusText}（${pngUrl}）`);
  }
  const png = new Uint8Array(await res.arrayBuffer());
  const src =
    key && key.length > 0 ? decode_png_with_key(png, key) : decode_png(png);

  const api = mode === 'script' ? compileAsScript(src) : await compileAsModule(src);
  const inst = new Js2Png(api, src);
  cache.set(cacheKey, inst);
  return inst;
}

/**
 * 预加载：页面启动即开始解密（推荐在页面入口第一时间调用）。
 * 返回与 load() 相同的 Promise；失败时打印错误而不产生未捕获异常。
 */
export function preload(pngUrl, key = null, opts = {}) {
  const p = load(pngUrl, key, opts);
  p.catch((e) => console.error('[js2png] 预加载失败：', e));
  return p;
}

/** module 模式：Blob + dynamic import，支持 export 语法 */
async function compileAsModule(src) {
  const url = URL.createObjectURL(new Blob([src], { type: 'text/javascript' }));
  try {
    return await import(/* webpackIgnore: true */ url);
  } finally {
    // 延迟回收，避免个别浏览器在 import 尚未完成时 revoke 导致失败
    setTimeout(() => URL.revokeObjectURL(url), 60_000);
  }
}

/** script 模式：作为普通脚本执行，业务代码按约定挂载到 window */
function compileAsScript(src) {
  const fn = new Function(`${src}\n;return (typeof window !== 'undefined') ? window : globalThis;`);
  return fn();
}
