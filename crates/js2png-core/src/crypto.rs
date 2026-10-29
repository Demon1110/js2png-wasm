//! 加密层：基于 SHA-256 的 XOR 计数器模式（CTR）密钥流
//!
//! 设计说明（安全边界，重要）：
//! - 本方案的目标是「前端代码交付保护」，属于**混淆级**保护：对抗静态扫描、
//!   自动爬取、正则提取与随手拷贝，而不是对抗有动机的专业逆向。
//! - 浏览器端最终必须拿到可执行的明文，因此任何纯前端方案都无法做到真正的保密。
//! - 密钥流 = SHA256(key || salt(8B) || counter(8B LE)) 逐块拼接，counter 递增。
//!   salt 随数据确定性派生（key + 长度），同一 key 与同一明文产出同一 PNG，便于交付可复现。
//! - 建议业务侧将 key 与 WASM 产物分开部署（如服务端下发），提高破解门槛。

use crate::sha256::sha256;

/// 内置默认密钥（仅作开箱即用的兜底；生产环境请通过 --key 显式指定并独立保管）
pub const DEFAULT_KEY: &[u8] = b"js2png-delivery-key-v1";

/// 由密钥与数据规模确定性派生 8 字节 salt（salt 明文写入容器头，不承担保密作用）
pub fn derive_salt(key: &[u8], original_len: usize, body_len: usize) -> [u8; 8] {
    let mut input = Vec::with_capacity(key.len() + 16);
    input.extend_from_slice(key);
    input.extend_from_slice(&(original_len as u64).to_le_bytes());
    input.extend_from_slice(&(body_len as u64).to_le_bytes());
    let digest = sha256(&input);
    let mut salt = [0u8; 8];
    salt.copy_from_slice(&digest[..8]);
    salt
}

/// XOR 计数器模式加密 / 解密（对称，同一函数）
///
/// keystream[i] = SHA256(key || salt || counter_le64)[i % 32]，counter 每 32 字节递增一次。
pub fn xor_ctr(data: &[u8], key: &[u8], salt: [u8; 8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    let mut counter: u64 = 0;
    let mut pos = 0usize;
    while pos < data.len() {
        let mut input = Vec::with_capacity(key.len() + 16);
        input.extend_from_slice(key);
        input.extend_from_slice(&salt);
        input.extend_from_slice(&counter.to_le_bytes());
        let ks = sha256(&input);
        for &b in &ks {
            if pos < data.len() {
                out.push(data[pos] ^ b);
            }
            pos += 1;
        }
        counter += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xor_ctr_roundtrip() {
        let data = "hello js2png, 中文内容也 OK".as_bytes();
        let key = b"test-key";
        let salt = [1u8, 2, 3, 4, 5, 6, 7, 8];
        let enc = xor_ctr(data, key, salt);
        let dec = xor_ctr(&enc, key, salt);
        assert_eq!(dec, data);
        // 加密结果不应与原数据相同（非空数据下）
        assert_ne!(enc, data);
    }

    #[test]
    fn xor_ctr_key_mismatch() {
        let data = b"secret payload";
        let enc = xor_ctr(data, b"key-a", [0u8; 8]);
        let dec = xor_ctr(&enc, b"key-b", [0u8; 8]);
        assert_ne!(dec, data);
    }

    #[test]
    fn xor_ctr_length_preserved() {
        for len in [0usize, 1, 31, 32, 33, 100] {
            let data = vec![7u8; len];
            assert_eq!(xor_ctr(&data, b"k", [9u8; 8]).len(), len);
        }
    }

    #[test]
    fn derive_salt_deterministic() {
        let a = derive_salt(b"k", 10, 20);
        let b = derive_salt(b"k", 10, 20);
        assert_eq!(a, b);
        let c = derive_salt(b"k2", 10, 20);
        assert_ne!(a, c);
    }
}
