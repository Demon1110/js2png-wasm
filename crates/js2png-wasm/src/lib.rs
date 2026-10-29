//! js2png-wasm：浏览器端反向解密绑定
//!
//! 通过 wasm-bindgen 导出给 JS 调用：
//! - decode_png(png: Uint8Array) -> string                无密钥解密（未加密数据）
//! - decode_png_with_key(png: Uint8Array, key: string) -> string
//! - encode_js(js: string) -> Uint8Array                  无密钥编码（仅压缩）
//! - encode_js_with_key(js: string, key: string) -> Uint8Array
//! - encode_js_with_key_expires(js: string, key: string, expires_in_secs: number) -> Uint8Array
//!     带有效期编码：从当前时刻起算，到期后任何解码调用都会失败
//!
//! 过期机制：编码时写入绝对过期时间（unix 秒）到容器头；解码时先取浏览器
//! 当前时间做过期检查（在解密之前），到期直接抛「已过期」错误，放弃解密。
//! 未带过期时间的旧图片不受影响（永不过期）。
//!
//! 浏览器加载示例见 web/ 目录演示页。安全提示见 core crate 文档。

use js2png_core::{decode_png_at as core_decode_at, encode_png as core_encode, EncodeOptions};
use wasm_bindgen::prelude::*;

/// 反向揭秘：PNG（无密钥编码）-> 原始 JS 源码字符串
///
/// 若容器带过期时间且已到期，在解密前直接失败。
#[wasm_bindgen]
pub fn decode_png(png: &[u8]) -> Result<String, JsValue> {
    let data = core_decode_at(png, None, Some(now_unix_secs())).map_err(to_js_err)?;
    bytes_to_string(data)
}

/// 反向揭秘：PNG（带密钥编码）-> 原始 JS 源码字符串
///
/// 若容器带过期时间且已到期，在解密前直接失败（即使密钥正确）。
#[wasm_bindgen]
pub fn decode_png_with_key(png: &[u8], key: &str) -> Result<String, JsValue> {
    let data = core_decode_at(png, Some(key.as_bytes()), Some(now_unix_secs())).map_err(to_js_err)?;
    bytes_to_string(data)
}

/// 正向编码：JS 源码 -> 加密 PNG（无密钥，仅压缩）
#[wasm_bindgen]
pub fn encode_js(js: &str) -> Result<Vec<u8>, JsValue> {
    let opts = EncodeOptions {
        key: None,
        width: None,
        height: None,
        compress: true,
        expire_at: None,
    };
    core_encode(js.as_bytes(), &opts).map_err(to_js_err)
}

/// 正向编码：JS 源码 -> 加密 PNG（带密钥）
#[wasm_bindgen]
pub fn encode_js_with_key(js: &str, key: &str) -> Result<Vec<u8>, JsValue> {
    let opts = EncodeOptions {
        key: Some(key.as_bytes()),
        width: None,
        height: None,
        compress: true,
        expire_at: None,
    };
    core_encode(js.as_bytes(), &opts).map_err(to_js_err)
}

/// 正向编码：JS 源码 -> 加密 PNG（带密钥 + 有效期）
///
/// `expires_in_secs`：有效时长（秒），从当前时刻起算；到期后解码将失败。
#[wasm_bindgen]
pub fn encode_js_with_key_expires(
    js: &str,
    key: &str,
    expires_in_secs: u32,
) -> Result<Vec<u8>, JsValue> {
    let expire_at = now_unix_secs().saturating_add(expires_in_secs as u64);
    let opts = EncodeOptions {
        key: Some(key.as_bytes()),
        width: None,
        height: None,
        compress: true,
        expire_at: Some(expire_at),
    };
    core_encode(js.as_bytes(), &opts).map_err(to_js_err)
}

/// 从 PNG 字节中读取图像宽高（Uint32Array [width, height]）
#[wasm_bindgen]
pub fn png_dimensions(png: &[u8]) -> Result<Vec<u32>, JsValue> {
    let img = js2png_core::png_dec::decode_png_rgb(png).map_err(to_js_err)?;
    Ok(vec![img.width, img.height])
}

/// 当前 unix 秒（UTC），取浏览器时钟（js_sys::Date::now()）
fn now_unix_secs() -> u64 {
    (js_sys::Date::now() / 1000.0) as u64
}

fn bytes_to_string(data: Vec<u8>) -> Result<String, JsValue> {
    String::from_utf8(data).map_err(|e| {
        to_js_err(format!(
            "解密结果不是合法的 UTF-8 文本（{}）——请确认密钥是否正确",
            e
        ))
    })
}

fn to_js_err<E: std::fmt::Display>(e: E) -> JsValue {
    JsValue::from_str(&e.to_string())
}
