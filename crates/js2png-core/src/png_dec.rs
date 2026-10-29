//! 手写 PNG 解码器：解析 chunk、CRC 校验、inflate、逆滤波（filter 0-4）
//!
//! 支持 color type 2（RGB）与 6（RGBA），bit depth 8，非隔行；其余组合明确报错。
//! 输出统一为 RGB24 像素流，供容器层提取嵌入数据。

use crate::crc32::crc32;
use crate::error::{Error, Result};

const PNG_SIGNATURE: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];

#[derive(Debug, Clone)]
pub struct PngImage {
    pub width: u32,
    pub height: u32,
    /// RGB24 像素流，长度 = width * height * 3
    pub rgb: Vec<u8>,
}

/// 解析 PNG 并输出 RGB24 像素
pub fn decode_png_rgb(data: &[u8]) -> Result<PngImage> {
    if data.len() < 8 || data[..8] != PNG_SIGNATURE {
        return Err(Error("不是有效的 PNG 文件（签名不匹配）".into()));
    }

    let mut pos = 8usize;
    let mut width: u32 = 0;
    let mut height: u32 = 0;
    let mut color_type: u8 = 0;
    let mut saw_ihdr = false;
    let mut idat = Vec::new();

    while pos + 12 <= data.len() {
        let len = u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        let total = pos + 12 + len;
        if total > data.len() {
            return Err(Error(format!(
                "PNG 数据截断：chunk 长度 {} 超出文件范围（offset {pos}）",
                len
            )));
        }
        let typ = &data[pos + 4..pos + 8];
        let chunk = &data[pos + 8..pos + 8 + len];

        // CRC 校验（type + data）
        let expected_crc =
            u32::from_be_bytes([data[pos + 8 + len], data[pos + 9 + len], data[pos + 10 + len], data[pos + 11 + len]]);
        let mut crc_input = Vec::with_capacity(4 + len);
        crc_input.extend_from_slice(typ);
        crc_input.extend_from_slice(chunk);
        if crc32(&crc_input) != expected_crc {
            let t = String::from_utf8_lossy(typ).to_string();
            return Err(Error(format!("PNG chunk CRC 校验失败：{t}（文件可能损坏）")));
        }

        match typ {
            b"IHDR" => {
                if chunk.len() != 13 {
                    return Err(Error("PNG IHDR 长度非法".into()));
                }
                width = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                height = u32::from_be_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]);
                let bit_depth = chunk[8];
                color_type = chunk[9];
                let _compression = chunk[10];
                let _filter_method = chunk[11];
                let interlace = chunk[12];
                if bit_depth != 8 {
                    return Err(Error(format!("不支持的位深：{bit_depth}（仅支持 8）")));
                }
                if color_type != 2 && color_type != 6 {
                    return Err(Error(format!(
                        "不支持的颜色类型：{color_type}（仅支持 2=RGB / 6=RGBA）"
                    )));
                }
                if interlace != 0 {
                    return Err(Error("不支持隔行（Adam7）PNG".into()));
                }
                if width == 0 || height == 0 {
                    return Err(Error("PNG 图像尺寸非法".into()));
                }
                saw_ihdr = true;
            }
            b"IDAT" => idat.extend_from_slice(chunk),
            b"IEND" => {
                // 数据结束
                break;
            }
            _ => {}
        }
        pos = total;
    }

    if !saw_ihdr {
        return Err(Error("PNG 缺少 IHDR chunk".into()));
    }
    if idat.is_empty() {
        return Err(Error("PNG 缺少 IDAT 图像数据".into()));
    }

    // zlib 解压
    let raw = miniz_oxide::inflate::decompress_to_vec_zlib(&idat)
        .map_err(|e| Error(format!("PNG 图像数据解压失败：{e}")))?;

    // 逆滤波还原扫描线
    let bpp: usize = if color_type == 2 { 3 } else { 4 };
    let stride = width as usize * bpp;
    let expect_raw = height as usize * (stride + 1);
    if raw.len() != expect_raw {
        return Err(Error(format!(
            "PNG 图像数据长度不符：期望 {expect_raw} 字节，实际 {} 字节",
            raw.len()
        )));
    }

    let mut rgb = Vec::with_capacity(width as usize * height as usize * 3);
    let mut prev = vec![0u8; stride];
    for y in 0..height as usize {
        let row_start = y * (stride + 1);
        let filter = raw[row_start];
        let row = &raw[row_start + 1..row_start + 1 + stride];
        let mut cur = vec![0u8; stride];
        match filter {
            0 => cur.copy_from_slice(row),
            1 => {
                for i in 0..stride {
                    let a = if i >= bpp { cur[i - bpp] } else { 0 };
                    cur[i] = row[i].wrapping_add(a);
                }
            }
            2 => {
                for i in 0..stride {
                    cur[i] = row[i].wrapping_add(prev[i]);
                }
            }
            3 => {
                for i in 0..stride {
                    let a = if i >= bpp { cur[i - bpp] } else { 0 };
                    let b = prev[i];
                    cur[i] = row[i].wrapping_add(((a as u16 + b as u16) / 2) as u8);
                }
            }
            4 => {
                for i in 0..stride {
                    let a = if i >= bpp { cur[i - bpp] } else { 0 };
                    let b = prev[i];
                    let c = if i >= bpp { prev[i - bpp] } else { 0 };
                    cur[i] = row[i].wrapping_add(paeth(a, b, c));
                }
            }
            other => {
                return Err(Error(format!("不支持的 PNG 行 filter：{other}")));
            }
        }

        if color_type == 2 {
            rgb.extend_from_slice(&cur);
        } else {
            // RGBA -> RGB，丢弃 alpha
            for px in 0..width as usize {
                let i = px * 4;
                rgb.extend_from_slice(&cur[i..i + 3]);
            }
        }
        prev = cur;
    }

    Ok(PngImage { width, height, rgb })
}

/// Paeth 预测器（PNG 规范 filter 4）
fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let p = a as i32 + b as i32 - c as i32;
    let pa = (p - a as i32).abs();
    let pb = (p - b as i32).abs();
    let pc = (p - c as i32).abs();
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造各种 filter 的手工 PNG（IHDR+IDAT+IEND）用于解码自检
    fn build_png(width: u32, height: u32, rows: &[Vec<u8>]) -> Vec<u8> {
        let mut raw = Vec::new();
        for row in rows {
            raw.extend_from_slice(row);
        }
        let idat = miniz_oxide::deflate::compress_to_vec_zlib(&raw, 9);
        let mut out = PNG_SIGNATURE.to_vec();
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&width.to_be_bytes());
        ihdr.extend_from_slice(&height.to_be_bytes());
        ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
        let mut crc_in = b"IHDR".to_vec();
        crc_in.extend_from_slice(&ihdr);
        out.extend_from_slice(&(ihdr.len() as u32).to_be_bytes());
        out.extend_from_slice(b"IHDR");
        out.extend_from_slice(&ihdr);
        out.extend_from_slice(&crc32(&crc_in).to_be_bytes());
        let mut crc_in2 = b"IDAT".to_vec();
        crc_in2.extend_from_slice(&idat);
        out.extend_from_slice(&(idat.len() as u32).to_be_bytes());
        out.extend_from_slice(b"IDAT");
        out.extend_from_slice(&idat);
        out.extend_from_slice(&crc32(&crc_in2).to_be_bytes());
        out.extend_from_slice(&0u32.to_be_bytes());
        out.extend_from_slice(b"IEND");
        out.extend_from_slice(&crc32(b"IEND").to_be_bytes());
        out
    }

    #[test]
    fn decode_filters_0_to_4() {
        let w = 4u32;
        let h = 3u32;
        let stride = (w as usize) * 3;
        // 每行用不同 filter 编码同一渐变数据
        let pixel: Vec<u8> = (0..(w as usize * h as usize * 3)).map(|i| i as u8).collect();
        let filters = [0u8, 1, 2, 3, 4];
        for (idx, f) in filters.iter().enumerate() {
            let mut rows = Vec::new();
            for y in 0..h as usize {
                let cur = &pixel[y * stride..(y + 1) * stride];
                let prev = if y > 0 { &pixel[(y - 1) * stride..y * stride] } else { &[0u8; 0] };
                let mut enc_row = Vec::with_capacity(1 + stride);
                enc_row.push(*f);
                match f {
                    0 => enc_row.extend_from_slice(cur),
                    1 => {
                        for i in 0..stride {
                            let a = if i >= 3 { cur[i - 3] } else { 0 };
                            enc_row.push(cur[i].wrapping_sub(a));
                        }
                    }
                    2 => {
                        for i in 0..stride {
                            let b = if y > 0 { prev[i] } else { 0 };
                            enc_row.push(cur[i].wrapping_sub(b));
                        }
                    }
                    3 => {
                        for i in 0..stride {
                            let a = if i >= 3 { cur[i - 3] } else { 0 };
                            let b = if y > 0 { prev[i] } else { 0 };
                            enc_row.push(cur[i].wrapping_sub(((a as u16 + b as u16) / 2) as u8));
                        }
                    }
                    4 => {
                        for i in 0..stride {
                            let a = if i >= 3 { cur[i - 3] } else { 0 };
                            let b = if y > 0 { prev[i] } else { 0 };
                            let c = if y > 0 && i >= 3 { prev[i - 3] } else { 0 };
                            enc_row.push(cur[i].wrapping_sub(paeth(a, b, c)));
                        }
                    }
                    _ => unreachable!(),
                }
                rows.push(enc_row);
            }
            let png = build_png(w, h, &rows);
            let img = decode_png_rgb(&png).expect(&format!("filter {f} 解码失败"));
            assert_eq!(img.width, w);
            assert_eq!(img.height, h);
            assert_eq!(img.rgb, pixel, "filter {idx} 还原不一致");
        }
    }

    #[test]
    fn decode_rgba_drops_alpha() {
        let w = 2u32;
        let h = 1u32;
        // RGBA: color type 6
        let mut raw = vec![0u8, 10, 20, 30, 255, 40, 50, 60, 99];
        // 手动构造 RGBA 类型 PNG
        let _ = &mut raw;
        let idat = miniz_oxide::deflate::compress_to_vec_zlib(&[0u8, 10, 20, 30, 255, 40, 50, 60, 99], 9);
        let mut out = PNG_SIGNATURE.to_vec();
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&w.to_be_bytes());
        ihdr.extend_from_slice(&h.to_be_bytes());
        ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
        let mut ci = b"IHDR".to_vec();
        ci.extend_from_slice(&ihdr);
        out.extend_from_slice(&(13u32).to_be_bytes());
        out.extend_from_slice(b"IHDR");
        out.extend_from_slice(&ihdr);
        out.extend_from_slice(&crc32(&ci).to_be_bytes());
        let mut ci2 = b"IDAT".to_vec();
        ci2.extend_from_slice(&idat);
        out.extend_from_slice(&(idat.len() as u32).to_be_bytes());
        out.extend_from_slice(b"IDAT");
        out.extend_from_slice(&idat);
        out.extend_from_slice(&crc32(&ci2).to_be_bytes());
        out.extend_from_slice(&0u32.to_be_bytes());
        out.extend_from_slice(b"IEND");
        out.extend_from_slice(&crc32(b"IEND").to_be_bytes());

        let img = decode_png_rgb(&out).unwrap();
        assert_eq!(img.rgb, vec![10, 20, 30, 40, 50, 60]);
    }

    #[test]
    fn decode_rejects_garbage() {
        assert!(decode_png_rgb(b"not a png").is_err());
        assert!(decode_png_rgb(&PNG_SIGNATURE).is_err());
    }

    #[test]
    fn decode_rejects_corrupted_crc() {
        let png = build_png(4, 2, &[vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12], vec![0, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24]]);
        let mut bad = png.clone();
        let last = bad.len() - 1;
        bad[last] ^= 0xFF; // 破坏 IEND CRC
        assert!(decode_png_rgb(&bad).is_err());
    }
}
