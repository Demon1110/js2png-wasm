# js2png-wasm

把 JS 源码加密嵌入 PNG 图片、并在浏览器端用 WebAssembly 反向解密还原执行的交付运行时。
图片本身可正常显示、可走 CDN，源码不随页面明文下发。

## 安装

```bash
npm i js2png-wasm
```

## 目录结构

```text
js2png-wasm/
├── package.json
├── js/
│   ├── runtime-sdk.js        # 客户运行时（本包入口）
│   ├── runtime-sdk.d.ts      # 类型声明
│   ├── main.js               # 演示页逻辑
│   └── runtime-demo-main.js  # 集成演示逻辑
├── wasm/                     # WASM 产物（wasm-pack 生成，必须随包一起发布）
│   ├── js2png_wasm.js
│   ├── js2png_wasm_bg.wasm
│   ├── js2png_wasm.d.ts
│   └── js2png_wasm_bg.wasm.d.ts
├── css/style.css
├── assets/demo-app.png      # 示例加密图（12×12，密钥 demo-key-2026）
├── index.html               # 演示页（编码 / 解密 / 执行）
└── runtime-demo.html        # 客户集成演示（预加载 + 就绪门闩）
```

## 快速开始

```js
import { preload, Js2Png } from 'js2png-wasm';

// ① 页面启动即预热：拉取 PNG + WASM 解密（不阻塞渲染）
export const bizReady = preload('./assets/app.png', '你的密钥');

// ② 业务入口 await 门闩：用户交互发生时必然已解密完成
export async function handleUserAction(name) {
  const biz = await bizReady;
  return biz.call('greet', name); // 或 biz.exports.greet(name)
}
```

业务代码两种约定：

- **ES Module（默认）**：`export function greet() {}` → `biz.call('greet')`、`biz.exports.version`
- **普通脚本**：业务代码挂 `window.MyApp = {...}` → `load(url, key, { mode: 'script' })`、`biz.call('MyApp.greet')`

## API

| 导出 | 说明 |
| --- | --- |
| `load(pngUrl, key?, opts?)` | 加载并解密，按 `pngUrl + key + mode` 幂等缓存，返回 `Promise<Js2Png>` |
| `preload(pngUrl, key?, opts?)` | 预热，返回同一个 Promise；失败只打印错误，不抛未捕获异常 |
| `Js2Png#call(path, ...args)` | 调用导出函数，支持 `'greet'` / `'MyApp.greet'` 点路径 |
| `Js2Png#exports` | 解密模块的导出对象（读常量用这里，`call` 只用于函数） |
| `Js2Png#source` | 解密出的原始源码 |

低阶 WASM 接口可通过子路径使用：

```js
import init, { encode_js, encode_js_with_key, decode_png, decode_png_with_key, png_dimensions }
  from 'js2png-wasm/wasm/js2png_wasm.js';
```

## 本地演示

```bash
python -m http.server 8765 --directory node_modules/js2png-wasm
# 打开 http://localhost:8765/index.html
# 客户集成演示 http://localhost:8765/runtime-demo.html
```

## 安全边界

这是**混淆级**交付保护：防静态扫描、防爬取，但浏览器端代码最终必须明文执行，无法对抗有动机的专业逆向。
密钥请与 WASM 产物分开下发（服务端按会话签发或环境变量注入），不要写死在仓库里。

## License

MIT
