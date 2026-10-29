// js2png-wasm 演示页逻辑：加载 WASM，串联编码 / 解码 / 页面脚本执行
import init, {
  encode_js,
  encode_js_with_key,
  decode_png,
  decode_png_with_key,
  png_dimensions,
} from './pkg/js2png_wasm.js';

const $ = (id) => document.getElementById(id);

const DEFAULT_SRC = `// 示例业务代码：点击下方「加密生成 PNG」，即可体验交付全流程
function greet(name) {
  const msg = \`你好，\${name}！本段代码已通过 js2png 加密嵌入 PNG，
并在浏览器中由 WASM 解密还原后执行。\`;
  console.log(msg);
  return msg;
}
document.getElementById('demo-effect').textContent = greet('World');
`;

let wasmReady = false;
let scriptExecutionId = 0;

/** 解析 PNG 头中的 IHDR 宽高（前端只读，不依赖 WASM） */
function pngSizeFromHeader(bytes) {
  const dv = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  return { width: dv.getUint32(16), height: dv.getUint32(20) };
}

function fmtBytes(n) {
  return n < 1024 ? `${n} B` : `${(n / 1024).toFixed(2)} KB`;
}

function setWasmStatus(text, cls) {
  const el = $('wasm-status');
  el.textContent = text;
  el.className = `badge ${cls}`;
}

async function main() {
  try {
    await init();
    wasmReady = true;
    setWasmStatus('WASM 已就绪', 'badge-ok');
    $('ver-info').textContent = '核心：SHA-256 / CRC32 / PNG 编解码手写实现';
  } catch (e) {
    setWasmStatus('WASM 加载失败', 'badge-err');
    console.error(e);
    return;
  }

  $('js-input').value = DEFAULT_SRC;

  // ---------- 编码 ----------
  $('btn-encode').addEventListener('click', () => {
    const src = $('js-input').value;
    if (!src.trim()) {
      $('enc-error').textContent = '请输入要加密的 JS 源码';
      return;
    }
    const key = $('enc-key').value.trim();
    try {
      const png = key
        ? encode_js_with_key(src, key)
        : encode_js(src);

      const { width, height } = pngSizeFromHeader(png);
      const sizeText = `PNG ${width}×${height} · ${fmtBytes(png.byteLength)} · ${key ? '已加密' : '仅压缩'}`;
      $('enc-meta').textContent = `JS ${fmtBytes(new TextEncoder().encode(src).length)} → ${sizeText}`;

      const blob = new Blob([png], { type: 'image/png' });
      const url = URL.createObjectURL(blob);
      $('png-preview').src = url;
      const dl = $('btn-download');
      dl.href = url;
      dl.download = key ? 'encrypted.png' : 'compressed.png';
      $('enc-result').hidden = false;
      $('enc-error').textContent = '';
    } catch (e) {
      $('enc-error').textContent = `编码失败：${e}`;
    }
  });

  $('btn-load-js').addEventListener('click', () => $('js-file').click());
  $('js-file').addEventListener('change', async (e) => {
    const f = e.target.files?.[0];
    if (!f) return;
    $('js-input').value = await f.text();
    e.target.value = '';
  });

  // ---------- 解码 ----------
  async function decodePng(bytes, name = '') {
    $('dec-error').textContent = '';
    const key = $('dec-key').value.trim();
    let src;
    if (key) {
      src = decode_png_with_key(bytes, key);
    } else {
      src = decode_png(bytes);
    }
    const [w, h] = png_dimensions(bytes);
    $('dec-meta').textContent =
      `${name ? name + ' · ' : ''}PNG ${w}×${h} · ${fmtBytes(bytes.byteLength)} · 还原 JS ${fmtBytes(new TextEncoder().encode(src).length)}`;
    const out = $('js-output');
    out.hidden = false;
    out.querySelector('code').textContent = src;
    $('btn-run').disabled = false;
    return src;
  }

  function handlePngFile(file) {
    file.arrayBuffer().then((buf) => {
      try {
        return decodePng(new Uint8Array(buf), file.name);
      } catch (e) {
        $('dec-error').textContent = `解密失败：${e}`;
        $('js-output').hidden = true;
        $('btn-run').disabled = true;
      }
    });
  }

  $('btn-decode').addEventListener('click', () => {
    const f = $('png-file').files?.[0];
    if (!f) return $('dec-error').textContent = '请先选择或拖入 PNG 文件';
    handlePngFile(f);
  });

  const drop = $('drop-zone');
  const pngFileInput = $('png-file');
  drop.addEventListener('click', () => pngFileInput.click());
  pngFileInput.addEventListener('change', (e) => {
    const f = e.target.files?.[0];
    if (f) handlePngFile(f);
    e.target.value = '';
  });
  ['dragover', 'dragenter'].forEach((ev) =>
    drop.addEventListener(ev, (e) => {
      e.preventDefault();
      drop.classList.add('dragover');
    })
  );
  ['dragleave', 'drop'].forEach((ev) =>
    drop.addEventListener(ev, (e) => {
      e.preventDefault();
      drop.classList.remove('dragover');
    })
  );
  drop.addEventListener('drop', (e) => {
    const f = e.dataTransfer?.files?.[0];
    if (f && f.type === 'image/png') handlePngFile(f);
  });

  // ---------- 作为页面脚本执行 ----------
  $('btn-run').addEventListener('click', async () => {
    const src = $('js-output').querySelector('code').textContent;
    const consoleEl = $('console-out');
    const body = $('console-body');
    body.textContent = '';
    consoleEl.hidden = false;

    const oldLog = console.log;
    const oldError = console.error;
    const lines = [];
    console.log = (...a) => lines.push(a.map(String).join(' '));
    console.error = (...a) => lines.push('[error] ' + a.map(String).join(' '));

    const onRuntimeError = (event) => {
      lines.push(`[exception] ${event.error?.stack || event.message}`);
      event.preventDefault();
      rejectExecution?.(new Error(event.message));
    };
    const onScriptError = () => rejectExecution?.(new Error('模块脚本解析或导入失败'));
    const completionEvent = `js2png-module-complete-${++scriptExecutionId}`;
    const script = document.createElement('script');
    script.type = 'module';
    script.textContent = `${src}\n;window.dispatchEvent(new Event(${JSON.stringify(completionEvent)}));`;
    let resolveExecution;
    let rejectExecution;

    try {
      await new Promise((resolve, reject) => {
        resolveExecution = resolve;
        rejectExecution = reject;
        window.addEventListener('error', onRuntimeError);
        window.addEventListener(completionEvent, resolveExecution, { once: true });
        script.addEventListener('error', onScriptError, { once: true });
        document.body.appendChild(script);
      });
    } catch (e) {
      if (!lines.some((line) => line.startsWith('[exception]'))) {
        lines.push(`[exception] ${e?.stack || e}`);
      }
    } finally {
      script.remove();
      window.removeEventListener('error', onRuntimeError);
      window.removeEventListener(completionEvent, resolveExecution);
      script.removeEventListener('error', onScriptError);
      resolveExecution = undefined;
      rejectExecution = undefined;
      console.log = oldLog;
      console.error = oldError;
    }
    body.textContent = lines.join('\n') || '脚本已在页面中运行（无 console 输出）';
  });
}

main();
