//! 容器层：定义嵌入 PNG 像素的字节布局
//!
//! ## 像素内布局（RGB24 通道，每像素 3 字节）
//! ```text
//! [0..4]      payload_len  u32 LE（container 字节数）
//! [4..4+L]    container（见下）
//! [4+L..]     PRNG 随机填充（视觉噪声，解码时忽略）
//! ```
//!
//! ## container 布局
//! ```text
//! [0..4]      magic  "J2PN"
//! [4]         version = 0x01
//! [5]         flags   bit0 = XOR 加密；bit1 = deflate 压缩
//! [6..10]     original_len  u32 LE（原始 JS 字节数）
//! [10..18]    salt   8 字节（加密时使用；未加密时为全 0）
//! [18..]      body   压缩后 / 加密后的数据
//! ```
//!
//! 处理顺序：原始 JS ->（可选）deflate 压缩 ->（可选）XOR 加密 -> container。
//! 解密顺序严格反向：XOR 解密 ->（可选）inflate -> 长度校验。

use crate::crypto::{derive_salt, xor_ctr};
use crate::error::{Error, Result};

pub const MAGIC: [u8; 4] = *b"J2PN";
pub const VERSION: u8 = 0x01;
pub const FLAG_ENCRYPTED: u8 = 0b0000_0001;
pub const FLAG_COMPRESSED: u8 = 0b0000_0010;
pub const HEADER_LEN: usize = 18;

/// 容器头信息（不包含 body）
#[derive(Debug, Clone)]
pub struct ContainerHeader {
    pub version: u8,
    pub flags: u8,
    pub original_len: usize,
    pub salt: [u8; 8],
}

impl ContainerHeader {
    pub fn encrypted(&self) -> bool {
        self.flags & FLAG_ENCRYPTED != 0
    }
    pub fn compressed(&self) -> bool {
        self.flags & FLAG_COMPRESSED != 0
    }
}

/// 打包 container：JS 字节 ->（压缩 -> 加密）-> container
pub fn build_container(js: &[u8], key: Option<&[u8]>, compress: bool) -> Vec<u8> {
    let mut flags: u8 = 0;
    if compress {
        flags |= FLAG_COMPRESSED;
    }
    if key.is_some() {
        flags |= FLAG_ENCRYPTED;
    }

    // 先压缩后加密（zlib 格式）
    let mut body = js.to_vec();
    if compress {
        body = miniz_oxide::deflate::compress_to_vec_zlib(&body, 9);
    }
    let salt: [u8; 8] = match key {
        Some(k) => derive_salt(k, js.len(), body.len()),
        None => [0u8; 8],
    };
    if let Some(k) = key {
        body = xor_ctr(&body, k, salt);
    }

    let mut out = Vec::with_capacity(HEADER_LEN + body.len());
    out.extend_from_slice(&MAGIC);
    out.push(VERSION);
    out.push(flags);
    out.extend_from_slice(&(js.len() as u32).to_le_bytes());
    out.extend_from_slice(&salt);
    out.extend_from_slice(&body);
    out
}

/// 仅解析 container 头（用于 info 命令，不解密不校验）
pub fn parse_header(buf: &[u8]) -> Result<ContainerHeader> {
    if buf.len() < HEADER_LEN {
        return Err(Error(format!(
            "数据过短（{} 字节，container 头至少需要 {HEADER_LEN} 字节）",
            buf.len()
        )));
    }
    if buf[..4] != MAGIC {
        return Err(Error("不是 js2png 生成的 PNG（缺少 J2PN 标识）".into()));
    }
    if buf[4] != VERSION {
        return Err(Error(format!("不支持的容器版本：{}", buf[4])));
    }
    let mut salt = [0u8; 8];
    salt.copy_from_slice(&buf[10..18]);
    Ok(ContainerHeader {
        version: buf[4],
        flags: buf[5],
        original_len: u32::from_le_bytes([buf[6], buf[7], buf[8], buf[9]]) as usize,
        salt,
    })
}

/// 解包 container：校验头 ->（解密 -> 解压）-> 原始 JS 字节
pub fn unpack_container(buf: &[u8], key: Option<&[u8]>) -> Result<(ContainerHeader, Vec<u8>)> {
    let header = parse_header(buf)?;

    let mut body = buf[HEADER_LEN..].to_vec();

    if header.encrypted() {
        let k = match key {
            Some(k) => k,
            None => {
                return Err(Error(
                    "数据已加密，需要提供密钥（CLI 使用 --key，WASM 使用 decode_png_with_key）".into(),
                ))
            }
        };
        body = xor_ctr(&body, k, header.salt);
    }

    if header.compressed() {
        body = miniz_oxide::inflate::decompress_to_vec_zlib(&body)
            .map_err(|e| Error(format!("解压失败（数据损坏或密钥错误）：{e}")))?;
    }

    if body.len() != header.original_len {
        return Err(Error(format!(
            "内容长度校验失败：预期 {} 字节，实际 {} 字节（可能密钥错误或文件损坏）",
            header.original_len,
            body.len()
        )));
    }

    Ok((header, body))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn container_roundtrip_no_key() {
        let js = b"function hello(){ console.log('hi'); }";
        let c = build_container(js, None, true);
        assert!(c[..4] == MAGIC);
        assert!(!parse_header(&c).unwrap().encrypted());
        let (_, out) = unpack_container(&c, None).unwrap();
        assert_eq!(out, js);
    }

    #[test]
    fn container_roundtrip_with_key() {
        let js = "中文代码：console.log('你好');".as_bytes();
        let c = build_container(js, Some(b"my-secret"), true);
        let h = parse_header(&c).unwrap();
        assert!(h.encrypted());
        assert!(h.compressed());
        let (_, out) = unpack_container(&c, Some(b"my-secret")).unwrap();
        assert_eq!(out, js);
    }

    #[test]
    fn container_wrong_key_fails() {
        let js = b"secret";
        let c = build_container(js, Some(b"right"), true);
        let err = unpack_container(&c, Some(b"wrong")).unwrap_err();
        assert!(err.to_string().contains("密钥") || err.to_string().contains("失败"));
    }

    #[test]
    fn container_encrypted_requires_key() {
        let c = build_container(b"x", Some(b"k"), true);
        let err = unpack_container(&c, None).unwrap_err();
        assert!(err.to_string().contains("密钥"));
    }

    #[test]
    fn container_rejects_bad_magic() {
        let c = build_container(b"x", None, false);
        let mut bad = c.clone();
        bad[0] = b'X';
        assert!(unpack_container(&bad, None).is_err());
    }

    #[test]
    fn container_no_compress_flag_respected() {
        let js = b"some js";
        let c = build_container(js, None, false);
        let h = parse_header(&c).unwrap();
        assert!(!h.compressed());
        let (_, out) = unpack_container(&c, None).unwrap();
        assert_eq!(out, js);
    }
}
