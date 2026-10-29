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
//! v2（当前）：
//! [0..4]      magic  "J2PN"
//! [4]         version = 0x02
//! [5]         flags   bit0 = XOR 加密；bit1 = deflate 压缩
//! [6..10]     original_len  u32 LE（原始 JS 字节数）
//! [10..18]    salt   8 字节（加密时使用；未加密时为全 0）
//! [18..26]    expire_at  u64 LE（unix 秒，0 = 永不过期）  —— v2 新增
//! [26..]      body   压缩后 / 加密后的数据
//! ```
//!
//! v1（旧版，仅兼容解析）：头长 18 字节，无 expire_at 字段，视为永不过期。
//! 旧版解码器遇到 v2 会因版本不匹配而拒绝，不会把过期字段误读为 body。
//!
//! 处理顺序：原始 JS ->（可选）deflate 压缩 ->（可选）XOR 加密 -> container。
//! 解密顺序严格反向：过期检查 -> XOR 解密 ->（可选）inflate -> 长度校验。

use crate::crypto::{derive_salt, xor_ctr};
use crate::error::{Error, Result};

pub const MAGIC: [u8; 4] = *b"J2PN";
pub const VERSION: u8 = 0x02;
/// 旧版容器版本（无过期字段，仅兼容解析）
pub const VERSION_V1: u8 = 0x01;
pub const FLAG_ENCRYPTED: u8 = 0b0000_0001;
pub const FLAG_COMPRESSED: u8 = 0b0000_0010;
/// v1 头长（无过期字段）
pub const HEADER_LEN_V1: usize = 18;
/// v2 头长（含 8 字节过期时间戳）
pub const HEADER_LEN_V2: usize = 26;

/// 容器头信息（不包含 body）
#[derive(Debug, Clone)]
pub struct ContainerHeader {
    pub version: u8,
    pub flags: u8,
    pub original_len: usize,
    pub salt: [u8; 8],
    /// 过期时间（unix 秒，UTC）；0 表示永不过期。v1 旧容器恒为 0。
    pub expire_at: u64,
}

impl ContainerHeader {
    pub fn encrypted(&self) -> bool {
        self.flags & FLAG_ENCRYPTED != 0
    }
    pub fn compressed(&self) -> bool {
        self.flags & FLAG_COMPRESSED != 0
    }
    /// 是否带过期时间（非 0 即为有效期限）
    pub fn has_expiry(&self) -> bool {
        self.expire_at != 0
    }
    /// 按给定当前时间（unix 秒）判断是否已过期
    pub fn expired_at(&self, now: u64) -> bool {
        self.has_expiry() && now >= self.expire_at
    }
}

/// 按版本返回头长度
fn header_len(version: u8) -> usize {
    match version {
        VERSION_V1 => HEADER_LEN_V1,
        _ => HEADER_LEN_V2,
    }
}

/// 打包 container：JS 字节 ->（压缩 -> 加密）-> container
///
/// `expire_at`：过期时间（unix 秒，UTC）；0 表示永不过期。
pub fn build_container(js: &[u8], key: Option<&[u8]>, compress: bool, expire_at: u64) -> Vec<u8> {
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

    let mut out = Vec::with_capacity(HEADER_LEN_V2 + body.len());
    out.extend_from_slice(&MAGIC);
    out.push(VERSION);
    out.push(flags);
    out.extend_from_slice(&(js.len() as u32).to_le_bytes());
    out.extend_from_slice(&salt);
    out.extend_from_slice(&expire_at.to_le_bytes());
    out.extend_from_slice(&body);
    out
}

/// 仅解析 container 头（用于 info 命令，不解密不校验）
pub fn parse_header(buf: &[u8]) -> Result<ContainerHeader> {
    if buf.len() < HEADER_LEN_V1 {
        return Err(Error(format!(
            "数据过短（{} 字节，container 头至少需要 {HEADER_LEN_V1} 字节）",
            buf.len()
        )));
    }
    if buf[..4] != MAGIC {
        return Err(Error("不是 js2png 生成的 PNG（缺少 J2PN 标识）".into()));
    }
    match buf[4] {
        VERSION => {
            if buf.len() < HEADER_LEN_V2 {
                return Err(Error(format!(
                    "v2 容器头需要 {HEADER_LEN_V2} 字节，实际仅 {} 字节（文件损坏）",
                    buf.len()
                )));
            }
            let mut salt = [0u8; 8];
            salt.copy_from_slice(&buf[10..18]);
            Ok(ContainerHeader {
                version: buf[4],
                flags: buf[5],
                original_len: u32::from_le_bytes([buf[6], buf[7], buf[8], buf[9]]) as usize,
                salt,
                expire_at: u64::from_le_bytes(
                    buf[18..26].try_into().expect("已校验长度"),
                ),
            })
        }
        VERSION_V1 => {
            let mut salt = [0u8; 8];
            salt.copy_from_slice(&buf[10..18]);
            Ok(ContainerHeader {
                version: buf[4],
                flags: buf[5],
                original_len: u32::from_le_bytes([buf[6], buf[7], buf[8], buf[9]]) as usize,
                salt,
                expire_at: 0, // v1 无过期字段，视为永不过期
            })
        }
        other => Err(Error(format!("不支持的容器版本：{other}"))),
    }
}

/// 解包 container：校验头 ->（过期检查 -> 解密 -> 解压）-> 原始 JS 字节
///
/// `now`：当前时间（unix 秒）。`None` 表示不做过期检查（兼容旧接口）；
/// `Some(now)` 时若容器带过期时间且 `now >= expire_at`，在解密前直接拒绝。
pub fn unpack_container(
    buf: &[u8],
    key: Option<&[u8]>,
    now: Option<u64>,
) -> Result<(ContainerHeader, Vec<u8>)> {
    let header = parse_header(buf)?;

    // 过期检查放在解密之前：到期即放弃解密，不产生任何解密计算
    if let Some(now) = now {
        if header.expired_at(now) {
            return Err(Error(format!(
                "加密图片已于 {}（unix 秒）过期，拒绝解密（当前 {}）",
                header.expire_at, now
            )));
        }
    }

    let mut body = buf[header_len(header.version)..].to_vec();

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
        let c = build_container(js, None, true, 0);
        assert!(c[..4] == MAGIC);
        assert!(!parse_header(&c).unwrap().encrypted());
        let (_, out) = unpack_container(&c, None, None).unwrap();
        assert_eq!(out, js);
    }

    #[test]
    fn container_roundtrip_with_key() {
        let js = "中文代码：console.log('你好');".as_bytes();
        let c = build_container(js, Some(b"my-secret"), true, 0);
        let h = parse_header(&c).unwrap();
        assert!(h.encrypted());
        assert!(h.compressed());
        let (_, out) = unpack_container(&c, Some(b"my-secret"), None).unwrap();
        assert_eq!(out, js);
    }

    #[test]
    fn container_wrong_key_fails() {
        let js = b"secret";
        let c = build_container(js, Some(b"right"), true, 0);
        let err = unpack_container(&c, Some(b"wrong"), None).unwrap_err();
        assert!(err.to_string().contains("密钥") || err.to_string().contains("失败"));
    }

    #[test]
    fn container_encrypted_requires_key() {
        let c = build_container(b"x", Some(b"k"), true, 0);
        let err = unpack_container(&c, None, None).unwrap_err();
        assert!(err.to_string().contains("密钥"));
    }

    #[test]
    fn container_rejects_bad_magic() {
        let c = build_container(b"x", None, false, 0);
        let mut bad = c.clone();
        bad[0] = b'X';
        assert!(unpack_container(&bad, None, None).is_err());
    }

    #[test]
    fn container_no_compress_flag_respected() {
        let js = b"some js";
        let c = build_container(js, None, false, 0);
        let h = parse_header(&c).unwrap();
        assert!(!h.compressed());
        let (_, out) = unpack_container(&c, None, None).unwrap();
        assert_eq!(out, js);
    }

    #[test]
    fn container_expiry_written_and_checked() {
        let js = b"expiring secret";
        let c = build_container(js, Some(b"k"), true, 1_700_000_000);
        let h = parse_header(&c).unwrap();
        assert_eq!(h.expire_at, 1_700_000_000);
        assert!(h.has_expiry());
        // 未过期：正常解密
        let (_, out) = unpack_container(&c, Some(b"k"), Some(1_699_999_999)).unwrap();
        assert_eq!(out, js);
        // 到期瞬间即失效
        assert!(unpack_container(&c, Some(b"k"), Some(1_700_000_000)).is_err());
        // 过期后：即使密钥正确也拒绝
        let err = unpack_container(&c, Some(b"k"), Some(1_800_000_000)).unwrap_err();
        assert!(err.to_string().contains("过期"));
    }

    #[test]
    fn container_no_expiry_never_expires() {
        let c = build_container(b"x", Some(b"k"), true, 0);
        let h = parse_header(&c).unwrap();
        assert_eq!(h.expire_at, 0);
        assert!(!h.has_expiry());
        // 任意时间戳都不受影响
        assert!(unpack_container(&c, Some(b"k"), Some(u64::MAX)).is_ok());
    }

    #[test]
    fn container_expiry_checked_before_decryption() {
        // 过期时无密钥也应报「过期」而不是「需要密钥」——说明检查发生在解密之前
        let c = build_container(b"x", Some(b"k"), true, 100);
        let err = unpack_container(&c, None, Some(200)).unwrap_err();
        assert!(err.to_string().contains("过期"));
        assert!(!err.to_string().contains("密钥"));
    }

    #[test]
    fn container_v1_header_compat() {
        // 手工构造 v1 容器（18 字节头，无过期字段，未加密未压缩）
        let mut buf = Vec::new();
        buf.extend_from_slice(&MAGIC);
        buf.push(VERSION_V1);
        buf.push(0); // flags
        buf.extend_from_slice(&4u32.to_le_bytes()); // original_len
        buf.extend_from_slice(&[0u8; 8]); // salt
        buf.extend_from_slice(b"abcd"); // body
        let h = parse_header(&buf).unwrap();
        assert_eq!(h.version, VERSION_V1);
        assert_eq!(h.expire_at, 0);
        assert!(!h.has_expiry());
        // v1 无过期字段，任意时间戳都不影响
        let (_, out) = unpack_container(&buf, None, Some(u64::MAX)).unwrap();
        assert_eq!(out, b"abcd");
    }

    #[test]
    fn container_v2_header_length_enforced() {
        let c = build_container(b"x", None, false, 123);
        // 截断到 v1 头长：v2 文件应被拒绝（不是文件损坏才怪）
        assert!(parse_header(&c[..HEADER_LEN_V1]).is_err());
    }
}
