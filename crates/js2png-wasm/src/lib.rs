//! js2png-wasm：浏览器端反向解密绑定
//!
//! 通过 wasm-bindgen 导出给 JS 调用：
//! - decode_png(png: Uint8Array) -> string                无密钥解密（未加密数据）
//! - decode_png_with_key(png: Uint8Array, key: string) -> string
//! - encode_js(js: string) -> Uint8Array                  无密钥编码（仅压缩）
//! - encode_js_with_key(js: string, key: string) -> Uint8Array
//!
//! 浏览器加载示例见 web/ 目录演示页。安全提示见 core crate 文档。

use js2png_core::{decode_png as core_decode, encode_png as core_encode, EncodeOptions};
use wasm_bindgen::prelude::*;

/// 反向揭秘：PNG（无密钥编码）-> 原始 JS 源码字符串
#[wasm_bindgen]
pub fn decode_png(png: &[u8]) -> Result<String, JsValue> {
    let data = core_decode(png, None).map_err(to_js_err)?;
    bytes_to_string(data)
}

/// 反向揭秘：PNG（带密钥编码）-> 原始 JS 源码字符串
#[wasm_bindgen]
pub fn decode_png_with_key(png: &[u8], key: &str) -> Result<String, JsValue> {
    let data = core_decode(png, Some(key.as_bytes())).map_err(to_js_err)?;
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
    };
    core_encode(js.as_bytes(), &opts).map_err(to_js_err)
}

/// 从 PNG 字节中读取图像宽高（Uint32Array [width, height]）
#[wasm_bindgen]
pub fn png_dimensions(png: &[u8]) -> Result<Vec<u32>, JsValue> {
    let img = js2png_core::png_dec::decode_png_rgb(png).map_err(to_js_err)?;
    Ok(vec![img.width, img.height])
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
