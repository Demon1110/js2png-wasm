// 客户集成演示逻辑：启动预热 + 就绪门闩 + 调用
import { preload, Js2Png } from './runtime-sdk.js';

const $ = (id) => document.getElementById(id);
const PNG_URL = './demo-app.png';
const KEY = 'demo-key-2026';

const t0 = performance.now();

// ① 页面启动即预热：立即开始 fetch + WASM 解密，不阻塞渲染
const ready = preload(PNG_URL, KEY);
// 业务入口拿到的永远是这个同一 Promise（幂等）

// ② 就绪门闩：页面展示用（await 同一 ready）
ready.then(() => {
  $('dot').className = 'dot ready';
  const ms = (performance.now() - t0).toFixed(0);
  $('status-text').textContent = `已就绪：解密完成并常驻内存（${ms} ms）`;
  $('btn-call').disabled = false;
  $('step-ready').textContent = `解密完成（${ms} ms）：ready Promise 已 fulfilled，业务代码常驻内存`;
}).catch((e) => {
  $('dot').className = 'dot failed';
  $('status-text').textContent = `预加载失败：${e}`;
});

// ③ 用户调用：await 门闩后立即执行（此时必然已就绪；极端情况同 Promise 兜底）
$('btn-call').addEventListener('click', async () => {
  const out = $('out');
  try {
    const biz = await ready;               // 已预热时零等待
    const ret = biz.call('greet', '客户'); // 调用解密模块的导出函数
    const ver = biz.exports.version;       // 读取值导出（call 只用于调用函数）
    out.textContent = `greet() 返回：${ret}\nversion：${ver}\n调用耗时：${(performance.now() - t0).toFixed(0)} ms（自页面启动）`;
    $('step-call').textContent = '用户调用：await ready 后立即执行，输出如上';
  } catch (e) {
    out.textContent = `调用失败：${e}`;
  }
});
