//! js2png-cli：JS 文件 <-> 加密 PNG 的命令行工具
//!
//! 用法：
//!   js2png-cli encode <input.js> <output.png> [--key <key>] [--width <w>] [--height <h>] [--no-compress]
//!   js2png-cli decode <input.png> <output.js> [--key <key>]
//!   js2png-cli info <input.png>
//!
//! 参数解析刻意手写，不引入 clap，保持交付二进制精简。

use std::env;
use std::fs;
use std::path::Path;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use js2png_core::{decode_png_at, encode_png, png_info, EncodeOptions};

const USAGE: &str = r#"js2png-cli v0.1.0 - JS 代码加密嵌入 PNG，支持反向解密还原

用法:
  js2png-cli encode <input.js> <output.png> [--key <key>] [--expires <seconds>] [--width <w>] [--height <h>] [--no-compress]
  js2png-cli decode <input.png> <output.js> [--key <key>] [--no-expire-check]
  js2png-cli info  <input.png>
  js2png-cli help

选项:
  --key <key>          加密密钥；不传则不加密（仅压缩）。encode/decode 需使用相同密钥
  --expires <seconds>  加密图片有效时长（秒，从编码时刻起算）；到期后 decode 将拒绝解密。
                       仅与 --key 一起使用才有意义（也可用于未加密图片，行为一致）
  --width <w>          指定 PNG 宽度（像素），不指定则自动计算接近正方形
  --height <h>         指定 PNG 高度（像素）
  --no-compress        关闭 deflate 压缩（默认开启）
  --no-expire-check    decode 时跳过过期检查（管理用途：调试、找回已过期数据）
"#;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        println!("{USAGE}");
        return ExitCode::from(2);
    }

    let result = match args[1].as_str() {
        "encode" => cmd_encode(&args[2..]),
        "decode" => cmd_decode(&args[2..]),
        "info" => cmd_info(&args[2..]),
        "help" | "--help" | "-h" => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        other => {
            eprintln!("未知命令：{other}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("错误：{msg}");
            ExitCode::from(1)
        }
    }
}

// ---------- 子命令 ----------

fn cmd_encode(args: &[String]) -> Result<(), String> {
    let mut key: Option<String> = None;
    let mut expires: Option<u64> = None;
    let mut width: Option<u32> = None;
    let mut height: Option<u32> = None;
    let mut compress = true;
    let mut positional: Vec<String> = Vec::new();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--key" => {
                i += 1;
                if i >= args.len() {
                    return Err("--key 缺少参数值".into());
                }
                key = Some(args[i].clone());
            }
            "--expires" => {
                i += 1;
                if i >= args.len() {
                    return Err("--expires 缺少参数值".into());
                }
                let secs = args[i]
                    .parse::<u64>()
                    .map_err(|_| format!("--expires 参数必须是正整数秒数，收到：{}", args[i]))?;
                if secs == 0 {
                    return Err("--expires 必须大于 0（0 会立即过期）".into());
                }
                expires = Some(secs);
            }
            "--width" => {
                i += 1;
                width = Some(parse_u32(&args, i, "--width")?);
            }
            "--height" => {
                i += 1;
                height = Some(parse_u32(&args, i, "--height")?);
            }
            "--no-compress" => compress = false,
            s if s.starts_with('-') => return Err(format!("未知选项：{s}")),
            s => positional.push(s.to_string()),
        }
        i += 1;
    }

    if positional.len() != 2 {
        return Err("encode 需要两个位置参数：<input.js> <output.png>".into());
    }
    let input = &positional[0];
    let output = &positional[1];

    let js = fs::read(input).map_err(|e| format!("读取 {input} 失败：{e}"))?;
    if js.is_empty() {
        return Err(format!("{input} 为空文件，无可编码内容"));
    }

    let expire_at = match expires {
        Some(secs) => Some(now_unix_secs()?.checked_add(secs).ok_or("过期时间超出 u64 范围")?),
        None => None,
    };

    let opts = EncodeOptions {
        key: key.as_deref().map(|s| s.as_bytes()),
        width,
        height,
        compress,
        expire_at,
    };
    let png = encode_png(&js, &opts).map_err(|e| e.to_string())?;
    fs::write(output, &png).map_err(|e| format!("写入 {output} 失败：{e}"))?;

    let meta = png_info(&png).map_err(|e| e.to_string())?;
    let hdr = &meta.header;
    println!(
        "编码完成：{} ({} 字节) -> {} ({} 字节)",
        Path::new(input).file_name().unwrap_or_default().to_string_lossy(),
        js.len(),
        output,
        png.len()
    );
    println!(
        "图像尺寸：{}x{}，{}，{}，原始 JS {} 字节",
        meta.width,
        meta.height,
        if hdr.encrypted() { "已加密" } else { "未加密" },
        if hdr.compressed() { "已压缩" } else { "未压缩" },
        hdr.original_len
    );
    if hdr.has_expiry() {
        let t = hdr.expire_at;
        let remain = t.saturating_sub(now_unix_secs()?);
        println!("有效至：{}（剩余约 {} 秒）", unix_to_utc_string(t), remain);
    }
    Ok(())
}

fn cmd_decode(args: &[String]) -> Result<(), String> {
    let mut key: Option<String> = None;
    let mut no_expire_check = false;
    let mut positional: Vec<String> = Vec::new();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--key" => {
                i += 1;
                if i >= args.len() {
                    return Err("--key 缺少参数值".into());
                }
                key = Some(args[i].clone());
            }
            "--no-expire-check" => no_expire_check = true,
            s if s.starts_with('-') => return Err(format!("未知选项：{s}")),
            s => positional.push(s.to_string()),
        }
        i += 1;
    }

    if positional.len() != 2 {
        return Err("decode 需要两个位置参数：<input.png> <output.js>".into());
    }
    let input = &positional[0];
    let output = &positional[1];

    let png = fs::read(input).map_err(|e| format!("读取 {input} 失败：{e}"))?;
    let now = if no_expire_check {
        None
    } else {
        Some(now_unix_secs()?)
    };
    let js = decode_png_at(&png, key.as_deref().map(|s| s.as_bytes()), now)
        .map_err(|e| e.to_string())?;
    fs::write(output, &js).map_err(|e| format!("写入 {output} 失败：{e}"))?;

    let meta = png_info(&png).map_err(|e| e.to_string())?;
    println!(
        "解密完成：{} ({} 字节) -> {} ({} 字节)",
        Path::new(input).file_name().unwrap_or_default().to_string_lossy(),
        png.len(),
        output,
        js.len()
    );
    println!(
        "图像 {}x{}，{}，{}，还原 JS {} 字节",
        meta.width,
        meta.height,
        if meta.header.encrypted() { "已加密" } else { "未加密" },
        if meta.header.compressed() { "已压缩" } else { "未压缩" },
        js.len()
    );
    Ok(())
}

fn cmd_info(args: &[String]) -> Result<(), String> {
    if args.len() != 1 || args[0].starts_with('-') {
        return Err("info 需要一个位置参数：<input.png>".into());
    }
    let input = &args[0];
    let png = fs::read(input).map_err(|e| format!("读取 {input} 失败：{e}"))?;
    let meta = png_info(&png).map_err(|e| e.to_string())?;
    let hdr = &meta.header;
    let now = now_unix_secs()?;
    println!("文件：{input}");
    println!("图像尺寸：{}x{}（像素）", meta.width, meta.height);
    println!("容器版本：v{}", hdr.version);
    println!("加密：{}", if hdr.encrypted() { "是（需要 --key 才能解密）" } else { "否" });
    println!("压缩：{}", if hdr.compressed() { "deflate" } else { "无" });
    println!("原始 JS 长度：{} 字节", hdr.original_len);
    if hdr.has_expiry() {
        println!(
            "过期时间：{}（{}）",
            unix_to_utc_string(hdr.expire_at),
            if hdr.expired_at(now) { "已过期" } else { "未过期" }
        );
    } else {
        println!("过期时间：永久有效");
    }
    Ok(())
}

// ---------- 工具 ----------

fn parse_u32(args: &[String], i: usize, name: &str) -> Result<u32, String> {
    if i >= args.len() {
        return Err(format!("{name} 缺少参数值"));
    }
    args[i]
        .parse::<u32>()
        .map_err(|_| format!("{name} 参数必须是正整数，收到：{}", args[i]))
}

/// 当前 unix 秒（UTC）
fn now_unix_secs() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .map_err(|e| format!("获取当前时间失败：{e}"))
}

/// unix 秒 -> "YYYY-MM-DD HH:MM:SS UTC"
///
/// 采用 Howard Hinnant 的 civil_from_days 算法，不引入外部依赖。
fn unix_to_utc_string(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (h, mi, s) = (rem / 3_600, (rem % 3_600) / 60, rem % 60);
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02} {h:02}:{mi:02}:{s:02} UTC")
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // day of era
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365; // year of era
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // day of year
    let mp = (5 * doy + 2) / 153; // month 3..14
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m as u32, d as u32)
}
