# 客户集成指南（INTEGRATION.md）

本文档回答两个问题：**客户拿到 PNG 图片后怎么在自己的项目里使用**，以及**怎么确保解密后的代码在用户调用之前就已经加载完毕**。

---

## 1. 客户拿到的物料清单

| 物料 | 说明 |
| --- | --- |
| `app.png`（示例为 `demo-app.png`） | 加密后的业务代码，可正常显示、可走 CDN |
| `js2png_wasm.js` + `js2png_wasm_bg.wasm` | WASM 解密引擎（`web/pkg/` 目录，整套拷贝） |
| `runtime-sdk.js` | 客户侧运行时封装：加载 PNG → 解密 → 编译 → 调用（**推荐直接使用**） |
| 密钥 | 加密时使用的 `--key`；与产物分开交付/下发 |

> 交付给客户的目录建议为：`js2png/`（内含 `pkg/`、`runtime-sdk.js`），加一个 `app.png`。明文业务代码**不要**随包交付。

---

## 2. 集成方式

### 2.1 方式 A：打包工具项目（Vite / Webpack / Rollup，推荐）

把 `js2png/` 目录复制到客户项目（例如 `src/js2png/`），然后正常 import：

```js
// 客户业务入口 app.js
import { preload, Js2Png } from './js2png/runtime-sdk.js';

const APP_PNG = new URL('./assets/app.png', import.meta.url).href; // 打包器会处理资源
const APP_KEY = '按你的密钥分发方案获取';                            // 见 §5

// ① 启动预热：页面初始化即开始解密（不阻塞渲染）
export const bizReady = preload(APP_PNG, APP_KEY);

// ② 业务入口：await 门闩，用户交互前必然就绪
export async function handleUserAction(name) {
  const biz = await bizReady;
  return biz.call('greet', name); // 或 biz.exports.greet(name)
}
```

关键点：`new URL(..., import.meta.url)` 让打包器正确内联/复制 PNG；`/* webpackIgnore: true */` 已写在 SDK 内，避免打包器误解析解密后代码的动态 import。

### 2.2 方式 B：原生 ESM（不用打包器）

项目结构与上面一致，直接用 `<script type="module">`：

```html
<script type="module">
  import { preload, Js2Png } from './js2png/runtime-sdk.js';
  window.bizReady = preload('./app.png', '密钥');
  window.startBiz = async (name) => (await window.bizReady).call('greet', name);
</script>
```

### 2.3 方式 C：传统 script 页面（无模块系统）

- 用打包器把 SDK 打成普通 UMD/IIFE 包，或
- 改用 `wasm-pack build --target no-modules` 产物 + SDK 的 `mode: 'script'`（业务代码约定挂 `window`，见 §4）。

---

## 3. 时序保障：确保「用户调用前已解密完成」（核心）

解密链路是异步的：`fetch PNG → WASM init → 解密 → import 编译`。用户的调用却通常是同步发生的。解决思路是**三层保障**，缺一不可：

### L1 启动即预热（关键）

页面入口处**立即**调用 `preload()`，不等 DOM 就绪、不等用户操作。浏览器在首帧渲染后的网络空闲期就会完成 fetch 与解密。预热不阻塞渲染——解密在后台进行。

```js
const ready = preload('./app.png', key); // 页面一加载就开始，Promise 立即返回
```

### L2 入口门闩（保证性措施）

所有真正执行业务代码的入口统一 `await ready`。由于 `load()`/`preload()` 幂等（同一参数返回**同一个 Promise**），无论预热是否完成，业务入口拿到的都是同一份就绪信号：

```js
async function onUserClick() {
  const biz = await ready;   // 已预热：微任务内立即返回；未完成：继续等待（兜底）
  biz.call('doSomething');
}
```

### L3 幂等兜底（极端情况）

如果业务代码在预热完成前就被触发（如首屏自动执行），`await ready` 会原地等待解密完成，**不会**重复加载、不会用未就绪的代码。这是比"检查标志位"更稳的做法——同一个 Promise 天然是状态机。

### 为什么这样能保证「用户调用前已加载」

- 用户能看到并点击页面 → 必然发生在**页面渲染之后**；
- 预热在**页面脚本执行瞬间**就已开始，网络与 CPU 竞速上早于任何用户交互；
- 即便用户手速极快（或首屏自动触发），L2 门闩也会等待解密完成后再执行业务逻辑；
- 结果：业务代码在内存中常驻（`Js2Png` 实例缓存），后续调用零等待。

### 补充优化

| 优化 | 做法 | 收益 |
| --- | --- | --- |
| 复用实例 | SDK 内置按 (url, key, mode) 缓存，重复 `load` 不重复解密 | 多入口共享一份 |
| HTTP 缓存 | PNG 走静态资源缓存（`Cache-Control: public, max-age`）；WASM 同理 | 二次访问免下载 |
| 持久缓存（可选） | 将解密结果存入 IndexedDB，二次会话直接恢复 | 首屏更快（自行实现） |
| 合并请求 | 小 PNG 可 base64 内联进 JS/HTML | 少一次往返 |

---

## 4. 业务代码编写约定（交付前定好）

| 模式 | 业务代码形态 | SDK 用法 |
| --- | --- | --- |
| `module`（推荐） | `export function greet(){}`、`export default {...}`、`export const x = ...` | `load(url, key)` → `biz.call('greet', ...)` / `biz.call('default.greet', ...)` / `biz.exports.greet(...)` |
| `script` | IIFE 挂全局：`window.MyApp = { greet(){} }` | `load(url, key, { mode: 'script' })` → `biz.call('MyApp.greet', ...)` |

注意：
- `module` 模式通过 Blob URL + `import()` 执行，支持完整 ESM 语法（含顶层 await、再 `import` 其它模块相对路径除外）；
- 业务代码**不要**依赖 `document.write` 等同步脚本行为；
- 如需引用外部库，建议在解密模块内用 `import()` 从 CDN 拉取，或由宿主页面先注入 `window`。

---

## 5. 密钥分发建议

- 密钥不要硬编码在页面可见位置（`<script>` 源码、localStorage）；
- 推荐：服务端接口按会话下发（如登录后 `GET /api/license` 返回），内存持有；
- 或：环境变量注入构建期（仅能防源码裸奔，无法防抓包）；
- 再次强调：本方案是**混淆级**保护，密钥+WASM 均可被专业逆向提取，请结合服务端鉴权使用。

---

## 6. 部署注意事项

| 事项 | 要求 |
| --- | --- |
| WASM MIME | 服务器对 `.wasm` 返回 `application/wasm`（Nginx：`types { application/wasm wasm; }`）。缺失时 wasm-bindgen 会降级为 `arrayBuffer` 加载，仍可运行但会告警 |
| CORS | PNG 与 WASM 跨域（CDN）时，响应需带 `Access-Control-Allow-Origin`；`fetch` 与 wasm instantiate 都需要 |
| 同源建议 | 简单场景把 PNG + `pkg/` + `runtime-sdk.js` 放同一目录/同域 |
| HTTPS | 生产环境必须 HTTPS（Blob URL import 与 wasm 无特例，但 fetch/隐私要求） |
| 版本 | 更新业务时重新加密 PNG 并更换密钥；PNG 文件名建议带版本号（`app-v2.png`）避免缓存旧版 |

---

## 7. 常见问题

**Q：解密后代码是字符串，怎么执行？**
SDK 已完成：`module` 模式用 Blob URL + `import()`（支持 export），`script` 模式用 `new Function` + window 约定。业务代码只需写标准 ESM/脚本，无需自管执行。

**Q：可以不预热，只在调用时加载吗？**
可以（`load()` 现场解密），但首次调用会有网络+解密延迟（几十~几百 ms）。预热 + 门闩是消除该延迟的标准做法。

**Q：页面有多个业务模块（多张 PNG）？**
对每个 PNG 各 `preload()` 一次；SDK 缓存按 URL+key 区分，互不影响。

**Q：解密失败/密钥错误会怎样？**
`load()` 抛错（中文描述），`preload()` 打印错误不产生未捕获异常；业务入口 `await ready` 会收到 rejection，建议 catch 后走降级提示（如"功能暂不可用"）。

**Q：Node/服务端（SSR）环境能用吗？**
WASM 可用于 Node（见 `scripts/smoke.mjs`），但 `fetch` 文件 URL 与 Blob URL import 是浏览器能力；SSR 场景请只在客户端执行解密，或改用 CLI 在构建期解密。

---

## 8. 本地演示

```powershell
python -m http.server 8765 --directory D:\java\git\js2png-wasm
# 打开 http://localhost:8765/web/runtime-demo.html
```

演示页展示：页面启动即预热 → 状态"已就绪（xx ms）"→ 点击按钮 `await ready` 后零等待调用解密出的业务函数。
