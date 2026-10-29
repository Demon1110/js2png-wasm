//! 手写 PNG 编码器：RGB24（color type 2, bit depth 8）非隔行 PNG
//!
//! 结构完全合规（IHDR/IDAT/IEND + CRC32），任何标准解码器（浏览器 / 图片软件）均可正常显示。
//! 刻意不写入 tEXt/iTXt 等任何元数据 chunk，避免在 PNG 结构中留下明显痕迹。
//! 像素行统一使用 filter=0（None），defalte 由 miniz_oxide 完成。

use crate::crc32::crc32;
use crate::error::{Error, Result};

const PNG_SIGNATURE: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];

/// 将 RGB24 像素数据编码为 PNG 文件字节
pub fn encode_rgb_png(width: u32, height: u32, rgb: &[u8]) -> Result<Vec<u8>> {
    let expect = (width as usize) * (height as usize) * 3;
    if rgb.len() != expect {
        return Err(Error(format!(
            "像素数据长度不符：期望 {} 字节（{width}x{height}），实际 {} 字节",
            expect,
            rgb.len()
        )));
    }

    // 每行前加 1 字节 filter 标记（0 = None）
    let stride = width as usize * 3;
    let mut raw = Vec::with_capacity(height as usize * (stride + 1));
    for y in 0..height as usize {
        raw.push(0);
        raw.extend_from_slice(&rgb[y * stride..(y + 1) * stride]);
    }

    // zlib 压缩（PNG IDAT 需要带 zlib 头的流）
    let idat = miniz_oxide::deflate::compress_to_vec_zlib(&raw, 9);

    let mut out = Vec::with_capacity(8 + 25 + 12 + idat.len() + 12);
    out.extend_from_slice(&PNG_SIGNATURE);

    // IHDR
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.push(8); // bit depth
    ihdr.push(2); // color type: truecolor RGB
    ihdr.push(0); // compression
    ihdr.push(0); // filter
    ihdr.push(0); // interlace: none
    write_chunk(&mut out, b"IHDR", &ihdr);

    write_chunk(&mut out, b"IDAT", &idat);
    write_chunk(&mut out, b"IEND", &[]);

    Ok(out)
}

fn write_chunk(out: &mut Vec<u8>, typ: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(typ);
    out.extend_from_slice(data);
    // CRC 覆盖 type + data
    let mut crc_input = Vec::with_capacity(4 + data.len());
    crc_input.extend_from_slice(typ);
    crc_input.extend_from_slice(data);
    out.extend_from_slice(&crc32(&crc_input).to_be_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::png_dec::decode_png_rgb;

    #[test]
    fn encode_produces_valid_png_and_roundtrip() {
        // 8x8 渐变噪声像素
        let mut rgb = Vec::new();
        for i in 0..(8usize * 8 * 3) {
            rgb.push((i as u8).wrapping_mul(13).wrapping_add(7));
        }
        let png = encode_rgb_png(8, 8, &rgb).unwrap();
        // 标准 PNG 签名
        assert_eq!(&png[..8], &PNG_SIGNATURE);
        let img = decode_png_rgb(&png).unwrap();
        assert_eq!(img.width, 8);
        assert_eq!(img.height, 8);
        assert_eq!(img.rgb, rgb);
    }

    #[test]
    fn encode_rejects_wrong_length() {
        let err = encode_rgb_png(4, 4, &[0u8; 10]).unwrap_err();
        assert!(err.to_string().contains("长度不符"));
    }

    #[test]
    fn encode_deterministic() {
        let rgb = vec![42u8; 3 * 16];
        let a = encode_rgb_png(4, 4, &rgb).unwrap();
        let b = encode_rgb_png(4, 4, &rgb).unwrap();
        assert_eq!(a, b);
    }
}
