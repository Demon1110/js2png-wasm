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

use js2png_core::{decode_png, encode_png, png_info, EncodeOptions};

const USAGE: &str = r#"js2png-cli v0.1.0 - JS 代码加密嵌入 PNG，支持反向解密还原

用法:
  js2png-cli encode <input.js> <output.png> [--key <key>] [--width <w>] [--height <h>] [--no-compress]
  js2png-cli decode <input.png> <output.js> [--key <key>]
  js2png-cli info  <input.png>
  js2png-cli help

选项:
  --key <key>         加密密钥；不传则不加密（仅压缩）。encode/decode 需使用相同密钥
  --width <w>         指定 PNG 宽度（像素），不指定则自动计算接近正方形
  --height <h>        指定 PNG 高度（像素）
  --no-compress       关闭 deflate 压缩（默认开启）
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

    let opts = EncodeOptions {
        key: key.as_deref().map(|s| s.as_bytes()),
        width,
        height,
        compress,
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
    Ok(())
}

fn cmd_decode(args: &[String]) -> Result<(), String> {
    let mut key: Option<String> = None;
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
    let js = decode_png(&png, key.as_deref().map(|s| s.as_bytes())).map_err(|e| e.to_string())?;
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
    println!("文件：{input}");
    println!("图像尺寸：{}x{}（像素）", meta.width, meta.height);
    println!("容器版本：v{}", hdr.version);
    println!("加密：{}", if hdr.encrypted() { "是（需要 --key 才能解密）" } else { "否" });
    println!("压缩：{}", if hdr.compressed() { "deflate" } else { "无" });
    println!("原始 JS 长度：{} 字节", hdr.original_len);
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
