use std::{env, path::PathBuf};

use trans_kun_lib::media::{create_proxy, sha256_file};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args_os().skip(1);
    let source = args
        .next()
        .map(PathBuf::from)
        .ok_or("usage: create_media_proxy <source-media-file> <app-data-media-directory>")?;
    let output_directory = args
        .next()
        .map(PathBuf::from)
        .ok_or("usage: create_media_proxy <source-media-file> <app-data-media-directory>")?;
    if args.next().is_some() {
        return Err("no extra arguments are accepted".into());
    }

    let proxy = create_proxy(&output_directory, &source)?;
    println!("proxy={}", proxy.path.display());
    println!("source_sha256={}", proxy.source_sha256);
    println!("proxy_sha256={}", sha256_file(&proxy.path)?);
    println!("proxy_bytes={}", std::fs::metadata(&proxy.path)?.len());
    println!("sample_count={}", proxy.sample_count);
    println!("duration_seconds={:.6}", proxy.duration_seconds);
    Ok(())
}
