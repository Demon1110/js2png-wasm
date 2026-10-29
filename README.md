# js2png-wasm

**前端代码加密交付方案**：把 JS 源码压缩、加密后嵌入 PNG 图片像素，交付端在浏览器中通过 WASM 反向解密还原并执行。

- 正向（交付端）：`JS 源码 → 加密 PNG`（CLI / 浏览器均可）
- 反向（揭秘端）：`PNG → JS 源码 → 执行`（浏览器 WASM，跨语言调用零成本）
- 生成的 PNG 是**完全合规的标准图片**（可正常显示、可被任意图片软件打开），像素呈彩色噪声，源码肉眼不可见、静态扫描与正则提取无效

## 特性

| 维度 | 说明 |
| --- | --- |
| 纯 Rust 全链路 | core / CLI / WASM 同一套核心逻辑，无 C 依赖 |
| 手写实现 | SHA-256、CRC32、PNG 编码器、PNG 解码器（filter 0-4 逆变换）均为手写；唯一第三方依赖 `miniz_oxide`（deflate） |
| 压缩 + 加密 | deflate 压缩 → XOR 计数器模式（SHA-256 密钥流），密钥可配置，支持不加密仅压缩模式 |
| 确定性输出 | 相同（源码, 密钥）产出相同 PNG，便于交付可复现、可缓存 |
| 完整性校验 | PNG chunk CRC 校验 + 容器长度校验 + 原始长度校验，损坏/错钥明确报错 |
| 多端交付 | CLI（Windows/Linux/Mac）、WASM（浏览器）、库（Rust） |

## 目录结构

```text
js2png-wasm/
├── Cargo.toml                  # Cargo workspace
├── crates/
│   ├── js2png-core/            # 核心库：编解码 + 加密 + 手写 PNG（纯 Rust，可编 wasm32）
│   │   └── src/
│   │       ├── lib.rs          # 公开 API：encode_png / decode_png / png_info
│   │       ├── container.rs    # 容器打包：magic/flags/长度/salt + body
│   │       ├── crypto.rs       # SHA-256 CTR 密钥流加密（对称）
│   │       ├── png_enc.rs      # 手写 PNG 编码器（RGB24）
│   │       ├── png_dec.rs      # 手写 PNG 解码器（filter 0-4）
│   │       ├── sha256.rs       # 手写 SHA-256
│   │       ├── crc32.rs        # 手写 CRC32
│   │       └── error.rs
│   ├── js2png-cli/             # 命令行工具（参数解析手写，无 clap）
│   │   └── src/main.rs         # encode / decode / info 三个子命令
│   └── js2png-wasm/            # WASM 绑定（wasm-bindgen，web target）
│       └── src/lib.rs          # decode_png / decode_png_with_key / encode_js / ...
├── web/                        # 浏览器演示页
│   ├── index.html / main.js / style.css
│   └── pkg/                    # wasm-pack 构建产物（git 忽略）
├── examples/demo.js            # 示例业务 JS
├── scripts/
│   ├── build-all.ps1           # 一键构建 + 全部测试（Windows）
│   ├── test-core.ps1           # 核心库单元测试
│   ├── build-cli.ps1           # CLI 发布构建
│   ├── build-wasm.ps1          # WASM 发布构建
│   ├── smoke-wasm.ps1          # WASM 冒烟测试
│   └── smoke.mjs               # Node 端 WASM 冒烟测试
└── dist/                       # CLI 示例产物（demo.png 等）
```

## 工作原理

### 像素内数据布局（RGB24 通道，每像素 3 字节，Alpha 恒为 255）

```text
[0..4]      payload_len   u32 LE  （container 字节数）
[4..4+L]    container     （见下）
[4+L..]     PRNG 填充      视觉噪声，解码时忽略
```

### container 布局（18 字节头 + body）

```text
[0..4]      magic      "J2PN"
[4]         version    0x01
[5]         flags      bit0 = XOR 加密；bit1 = deflate 压缩
[6..10]     orig_len   u32 LE（原始 JS 字节数）
[10..18]    salt       8 字节（密钥流种子；未加密时为 0）
[18..]      body       压缩后 / 加密后的数据
```

### 数据流

```text
编码（正向）: JS ──deflate──> body ──XOR CTR──> container ──嵌入像素──> PNG
解码（反向）: PNG ──解析像素──> container ──XOR 解密──> inflate ──长度校验──> JS
```

加密处理顺序为“先压缩、后加密”；解密严格反向。密钥错误会在 inflate 或长度校验阶段被捕获并明确报错。

## 快速开始

### 环境要求

- Rust 1.70+（含 `wasm32-unknown-unknown` target：`rustup target add wasm32-unknown-unknown`）
- [wasm-pack](https://rustwasm.github.io/wasm-pack/) 0.12+
- Node.js 18+（仅冒烟测试与演示需要）

### 一键构建与测试（Windows）

```powershell
pwsh -File scripts\build-all.ps1
```

四个独立运行命令：

```powershell
pwsh -File scripts\test-core.ps1
pwsh -File scripts\build-cli.ps1
pwsh -File scripts\build-wasm.ps1
pwsh -File scripts\smoke-wasm.ps1
```

### CLI 用法

```text
js2png-cli encode <input.js> <output.png> [--key <key>] [--width <w>] [--height <h>] [--no-compress]
js2png-cli decode <input.png> <output.js> [--key <key>]
js2png-cli info  <input.png>
```

示例：

```bash
# 加密交付（推荐：使用独立密钥，密钥单独保管）
js2png-cli encode examples/demo.js dist/demo.png --key "my-secret-key-2026"

# 查看 PNG 内嵌元信息（无需密钥）
js2png-cli info dist/demo.png

# 本地解密验证（密钥必须与编码一致）
js2png-cli decode dist/demo.png dist/demo.decoded.js --key "my-secret-key-2026"
```

### 浏览器演示页

```bash
python -m http.server 8765 --directory D:\java\git\js2png-wasm
# 浏览器打开 http://localhost:8765/web/
```

演示页支持：JS → PNG 编码（预览 + 下载）、PNG → JS 解密（拖拽 / 选择文件）、一键执行解密出的代码。

## WASM API

```ts
// 均在 init() 完成后可用（见 web/main.js 与 scripts/smoke.mjs）
decode_png(png: Uint8Array): string            // 解密未加密数据（仅压缩）
decode_png_with_key(png: Uint8Array, key: string): string  // 解密带密钥数据
encode_js(js: string): Uint8Array              // 浏览器端编码（仅压缩）
encode_js_with_key(js: string, key: string): Uint8Array    // 浏览器端编码（带密钥）
png_dimensions(png: Uint8Array): Uint32Array   // 读取 PNG 宽高
```

失败时抛 `Error`，消息为中文可读描述（如“数据已加密，需要提供密钥”“解压失败（数据损坏或密钥错误）”）。

## 安全边界（务必阅读）

本方案是**混淆级**的前端交付保护，不是强加密：

- 防护对象：自动爬取、静态扫描、正则提取、随手拷贝、绕过审查的明文分发
- 无法对抗：有动机的专业逆向。浏览器最终必须拿到明文执行，WASM 字节码与密钥均可被逆向与提取
- 建议做法：**密钥与产物分开部署**（如服务端按会话下发密钥）；密钥轮换；核心敏感逻辑尽量留在服务端
- 加密算法（XOR-CTR + SHA-256 密钥流）为工程混淆设计，非审计级密码学方案

## 测试覆盖

| 层 | 手段 | 覆盖 |
| --- | --- | --- |
| 核心库 | `cargo test`（28 项） | SHA-256 标准向量、CRC32 标准值、PNG 5 种 filter 逆变换、RGBA 抽取、容器往返（无密钥/带密钥/空文件/20 万字节/自定义尺寸）、错钥报错、损坏 CRC 报错、确定性输出 |
| CLI | 端到端 | encode → info → decode 往返字节一致；错钥退出码 1；System.Drawing 独立解码验证 PNG 合法 |
| WASM | `scripts/smoke.mjs`（8 项） | 编码/解密往返、错钥异常、加密数据缺钥异常、key 参数对未加密数据无副作用 |
| 浏览器 | 真实浏览器验证 | WASM 加载、浏览器端编码预览、上传 CLI 加密 PNG 解密还原、执行解密代码输出正确 |

## 技术细节

- **PNG 编码**：RGB24、bit depth 8、filter 0，IDAT 使用 zlib 流，IHDR/IDAT/IEND 逐 chunk 计算 CRC32；不写任何 tEXt/iTXt 元数据，避免留下嵌入痕迹
- **PNG 解码**：完整解析 chunk 结构并校验 CRC，支持 color type 2/6、bit depth 8、非隔行；实现全部 5 种行 filter 逆变换（Sub/Up/Average/Paeth）
- **密钥流**：`keystream[i] = SHA256(key || salt(8B) || counter(8B LE))[i % 32]`，counter 每 32 字节递增；salt 由 key + 数据规模确定性派生
- **图像尺寸**：默认按负载长度取近似正方形；支持 `--width/--height` 指定，容量不足时明确报错
- **确定性填充**：空白像素由基于容器哈希种子的 xorshift64 生成，同输入同输出

## License

MIT
