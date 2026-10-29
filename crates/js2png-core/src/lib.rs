//! js2png-core：JS 代码 <-> 加密 PNG 的核心库（纯 Rust，可编译到 wasm32）
//!
//! 正向（交付）：`encode_png` —— JS 源码 -> deflate 压缩 -> XOR 加密 -> 嵌入 PNG 像素 RGB 通道
//! 反向（揭秘）：`decode_png` —— PNG -> 解析像素 -> 提取容器 -> XOR 解密 -> inflate -> 原始 JS
//!
//! 手写实现：SHA-256、CRC32、PNG 编码器、PNG 解码器（filter 0-4）；唯一第三方依赖 miniz_oxide（deflate）。
//!
//! ## 安全边界
//! 这是**混淆级**的交付保护（防静态扫描 / 防爬取 / 防随手拷贝），不是强加密：
//! 浏览器最终必须拿到明文执行，WASM 与密钥均可被有动机的专业逆向。请把密钥与产物分离部署。

pub mod container;
pub mod crc32;
pub mod crypto;
pub mod error;
pub mod png_dec;
pub mod png_enc;
pub mod sha256;

pub use container::{ContainerHeader, MAGIC, VERSION};
pub use error::{Error, Result};

/// 编码选项
#[derive(Debug, Clone, Default)]
pub struct EncodeOptions<'a> {
    /// 加密密钥；`None` 表示不加密（仅压缩）
    pub key: Option<&'a [u8]>,
    /// 指定 PNG 宽度（像素）；不指定时自动计算接近正方形
    pub width: Option<u32>,
    /// 指定 PNG 高度（像素）
    pub height: Option<u32>,
    /// 是否启用 deflate 压缩，默认 true
    pub compress: bool,
    /// 过期时间（unix 秒，UTC）；`None` 表示永不过期。
    /// 带有效期编码的 PNG 到期后，解码端（decode_png_at / WASM / CLI）将拒绝解密。
    pub expire_at: Option<u64>,
}

impl<'a> EncodeOptions<'a> {
    pub fn new() -> Self {
        EncodeOptions {
            key: None,
            width: None,
            height: None,
            compress: true,
            expire_at: None,
        }
    }
}

/// 正向：JS 源码 -> 加密 PNG 文件字节
pub fn encode_png(js: &[u8], opts: &EncodeOptions) -> Result<Vec<u8>> {
    if opts.compress == false && opts.key.is_none() {
        // 允许，只是不加密不压缩的裸嵌入
    }

    let container = container::build_container(js, opts.key, opts.compress, opts.expire_at.unwrap_or(0));
    let payload_total = 4 + container.len(); // 4 字节负载长度 + container
    let need_pixels = ceil_div(payload_total, 3);
    let (w, h) = choose_size(need_pixels, opts.width, opts.height)?;

    let capacity = (w as usize) * (h as usize) * 3;
    if capacity < payload_total {
        return Err(Error(format!(
            "图像容量不足：{w}x{h} 仅可容纳 {capacity} 字节，需要至少 {payload_total} 字节"
        )));
    }

    let mut rgb = vec![0u8; capacity];
    rgb[0..4].copy_from_slice(&(container.len() as u32).to_le_bytes());
    rgb[4..payload_total].copy_from_slice(&container);

    // 空白像素用确定性 PRNG 填充（视觉噪声，可复现）
    let seed = u64::from_le_bytes(crate::sha256::sha256(&container)[..8].try_into().unwrap()) | 1;
    let mut rng = Xorshift64(seed);
    for i in payload_total..capacity {
        rgb[i] = rng.next() as u8;
    }

    png_enc::encode_rgb_png(w, h, &rgb)
}

/// 反向：PNG 文件字节 -> 原始 JS 源码字节
///
/// 兼容旧接口：不做过期检查（等价于 `decode_png_at(..., None)`）。
/// 需要过期校验时请使用 [`decode_png_at`]。
pub fn decode_png(png: &[u8], key: Option<&[u8]>) -> Result<Vec<u8>> {
    decode_png_at(png, key, None)
}

/// 反向：PNG 文件字节 -> 原始 JS 源码字节（带过期检查）
///
/// `now`：当前时间（unix 秒）。容器带过期时间且 `now >= expire_at` 时，
/// 在解密之前直接返回「已过期」错误，放弃解密。
/// 传 `None` 可跳过过期检查。
pub fn decode_png_at(png: &[u8], key: Option<&[u8]>, now: Option<u64>) -> Result<Vec<u8>> {
    let img = png_dec::decode_png_rgb(png)?;
    let rgb = &img.rgb;
    if rgb.len() < 4 {
        return Err(Error("PNG 图像过小，不包含有效负载".into()));
    }
    let payload_len = u32::from_le_bytes([rgb[0], rgb[1], rgb[2], rgb[3]]) as usize;
    if payload_len < container::HEADER_LEN_V1 {
        return Err(Error(format!(
            "无效负载长度：{payload_len}（这不是 js2png 生成的 PNG）"
        )));
    }
    if 4 + payload_len > rgb.len() {
        return Err(Error(format!(
            "负载长度 {payload_len} 超出图像容量 {}（文件损坏）",
            rgb.len() - 4
        )));
    }
    let (_, data) = container::unpack_container(&rgb[4..4 + payload_len], key, now)?;
    Ok(data)
}

/// 仅读取 PNG 与容器元信息（不要求密钥，不解密 body）
#[derive(Debug, Clone)]
pub struct PngMeta {
    pub width: u32,
    pub height: u32,
    pub header: ContainerHeader,
}

pub fn png_info(png: &[u8]) -> Result<PngMeta> {
    let img = png_dec::decode_png_rgb(png)?;
    let rgb = &img.rgb;
    if rgb.len() < 4 {
        return Err(Error("PNG 图像过小，不包含有效负载".into()));
    }
    let payload_len = u32::from_le_bytes([rgb[0], rgb[1], rgb[2], rgb[3]]) as usize;
    if payload_len < container::HEADER_LEN_V1 || 4 + payload_len > rgb.len() {
        return Err(Error("不是 js2png 生成的 PNG（负载长度非法）".into()));
    }
    let header = container::parse_header(&rgb[4..4 + payload_len])?;
    Ok(PngMeta {
        width: img.width,
        height: img.height,
        header,
    })
}

// ---------- 内部工具 ----------

fn ceil_div(a: usize, b: usize) -> usize {
    a / b + if a % b != 0 { 1 } else { 0 }
}

/// 计算容纳 need_pixels 个像素的图像尺寸
fn choose_size(need_pixels: usize, width: Option<u32>, height: Option<u32>) -> Result<(u32, u32)> {
    match (width, height) {
        (Some(w), Some(h)) => {
            if (w as usize) * (h as usize) < need_pixels {
                return Err(Error(format!(
                    "指定尺寸 {w}x{h} 容量不足：需要至少 {need_pixels} 像素"
                )));
            }
            Ok((w, h))
        }
        (Some(w), None) => {
            let w = w as usize;
            let h = ceil_div(need_pixels, w);
            Ok((w as u32, h as u32))
        }
        (None, Some(h)) => {
            let h = h as usize;
            let w = ceil_div(need_pixels, h);
            Ok((w as u32, h as u32))
        }
        (None, None) => {
            let w = (need_pixels as f64).sqrt().ceil().max(1.0) as usize;
            let h = ceil_div(need_pixels, w);
            Ok((w as u32, h as u32))
        }
    }
}

/// xorshift64 伪随机（仅用于像素填充噪声，不承担安全职责）
struct Xorshift64(u64);

impl Xorshift64 {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip_with(js: &[u8], key: Option<&[u8]>, w: Option<u32>, h: Option<u32>) {
        let opts = EncodeOptions {
            key,
            width: w,
            height: h,
            compress: true,
            expire_at: None,
        };
        let png = encode_png(js, &opts).expect("encode 失败");
        // PNG 签名合法
        assert_eq!(&png[..8], &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);
        let out = decode_png(&png, key).expect("decode 失败");
        assert_eq!(out, js, "roundtrip 不一致");

        let info = png_info(&png).unwrap();
        assert!(info.width * info.height >= ceil_div(4 + 18, 3) as u32);
    }

    #[test]
    fn full_roundtrip_plain() {
        roundtrip_with(b"console.log('hello');", None, None, None);
    }

    #[test]
    fn full_roundtrip_encrypted() {
        roundtrip_with(
            "const a = 1; // 中文注释\nfunction f(){ return a; }".as_bytes(),
            Some(b"top-secret-key"),
            None,
            None,
        );
    }

    #[test]
    fn full_roundtrip_empty() {
        roundtrip_with(b"", None, None, None);
    }

    #[test]
    fn full_roundtrip_large() {
        let js: Vec<u8> = (0..200_000).map(|i| (i % 251) as u8).collect();
        roundtrip_with(&js, Some(b"k"), None, None);
    }

    #[test]
    fn full_roundtrip_custom_size() {
        roundtrip_with(b"fixed size test", None, Some(64), Some(64));
        // 指定宽，自适应高
        roundtrip_with(b"width fixed", None, Some(32), None);
    }

    #[test]
    fn decode_wrong_key_fails() {
        let opts = EncodeOptions {
            key: Some(b"right"),
            width: None,
            height: None,
            compress: true,
            expire_at: None,
        };
        let png = encode_png(b"needle", &opts).unwrap();
        let err = decode_png(&png, Some(b"wrong")).unwrap_err();
        assert!(err.to_string().contains("密钥") || err.to_string().contains("失败"));
        // 加密数据不传密钥也要报错
        assert!(decode_png(&png, None).is_err());
    }

    #[test]
    fn decode_random_png_fails() {
        // 随机字节构成"PNG"但无负载
        let opts = EncodeOptions {
            key: None,
            width: None,
            height: None,
            compress: true,
            expire_at: None,
        };
        let png = encode_png(b"x", &opts).unwrap();
        let mut tampered = png.clone();
        let n = tampered.len();
        tampered[n - 1] ^= 0x01; // 破坏 IEND CRC
        assert!(decode_png(&tampered, None).is_err());
    }

    #[test]
    fn deterministic_output() {
        let opts = EncodeOptions {
            key: Some(b"k"),
            width: None,
            height: None,
            compress: true,
            expire_at: None,
        };
        let a = encode_png(b"same", &opts).unwrap();
        let b = encode_png(b"same", &opts).unwrap();
        assert_eq!(a, b, "相同输入应产出确定性输出");
    }

    #[test]
    fn info_reports_meta() {
        let opts = EncodeOptions {
            key: Some(b"k"),
            width: Some(20),
            height: Some(20),
            compress: true,
            expire_at: None,
        };
        let png = encode_png(b"meta test", &opts).unwrap();
        let info = png_info(&png).unwrap();
        assert_eq!((info.width, info.height), (20, 20));
        assert!(info.header.encrypted());
        assert!(info.header.compressed());
        assert_eq!(info.header.original_len, 9);
        assert!(!info.header.has_expiry());
    }

    #[test]
    fn encode_with_expiry_and_decode_at() {
        let opts = EncodeOptions {
            key: Some(b"k"),
            width: None,
            height: None,
            compress: true,
            expire_at: Some(1_700_000_000),
        };
        let png = encode_png(b"expiring payload", &opts).unwrap();
        let info = png_info(&png).unwrap();
        assert_eq!(info.header.expire_at, 1_700_000_000);

        // 过期前可解密
        assert_eq!(
            decode_png_at(&png, Some(b"k"), Some(1_699_999_999)).unwrap(),
            b"expiring payload"
        );
        // 到期瞬间拒绝
        assert!(decode_png_at(&png, Some(b"k"), Some(1_700_000_000)).is_err());
        // 过期后即使密钥正确也拒绝
        let err = decode_png_at(&png, Some(b"k"), Some(2_000_000_000)).unwrap_err();
        assert!(err.to_string().contains("过期"));
        // 旧接口 decode_png（now=None）不做过期检查，仍可解（兼容行为）
        assert_eq!(decode_png(&png, Some(b"k")).unwrap(), b"expiring payload");
    }
}
