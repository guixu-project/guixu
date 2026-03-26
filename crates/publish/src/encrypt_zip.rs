use anyhow::{Context, Result};
use std::io::{Cursor, Write};
use std::path::Path;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

/// Encrypt and compress a file into an AES-256 encrypted zip archive in memory.
/// Returns the zip bytes.
pub fn encrypt_zip(path: &Path, password: &str) -> Result<Vec<u8>> {
    let data = std::fs::read(path).context("read source file")?;
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("data");

    let buf = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(buf);

    let opts = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .with_aes_encryption(zip::AesMode::Aes256, password);

    zip.start_file(file_name, opts)?;
    zip.write_all(&data)?;

    let cursor = zip.finish()?;
    Ok(cursor.into_inner())
}
