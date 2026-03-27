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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn roundtrip_encrypt_decrypt() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.csv");
        std::fs::write(&file_path, "id,name\n1,alice\n2,bob\n").unwrap();

        let zip_bytes = encrypt_zip(&file_path, "s3cret").unwrap();
        assert!(zip_bytes.len() > 4);
        assert_eq!(&zip_bytes[0..2], b"PK");

        let reader = Cursor::new(&zip_bytes);
        let mut archive = zip::ZipArchive::new(reader).unwrap();
        assert_eq!(archive.len(), 1);
        let mut f = archive.by_index_decrypt(0, b"s3cret").unwrap();
        let mut content = String::new();
        f.read_to_string(&mut content).unwrap();
        assert_eq!(content, "id,name\n1,alice\n2,bob\n");
    }

    #[test]
    fn wrong_password_fails() {
        let dir = tempfile::tempdir().unwrap();
        let fp = dir.path().join("data.txt");
        std::fs::write(&fp, "secret").unwrap();

        let zip_bytes = encrypt_zip(&fp, "correct").unwrap();
        let reader = Cursor::new(&zip_bytes);
        let mut archive = zip::ZipArchive::new(reader).unwrap();
        // Wrong password: by_index_decrypt returns Err or read fails
        let result = archive.by_index_decrypt(0, b"wrong");
        if let Ok(mut f) = result {
            let mut buf = Vec::new();
            // Read should fail with wrong AES key
            assert!(f.read_to_end(&mut buf).is_err() || buf != b"secret");
        }
    }

    #[test]
    fn preserves_filename() {
        let dir = tempfile::tempdir().unwrap();
        let fp = dir.path().join("my_dataset.parquet");
        std::fs::write(&fp, b"data").unwrap();

        let zip_bytes = encrypt_zip(&fp, "pw").unwrap();
        let reader = Cursor::new(&zip_bytes);
        let archive = zip::ZipArchive::new(reader).unwrap();
        assert_eq!(archive.name_for_index(0).unwrap(), "my_dataset.parquet");
    }

    #[test]
    fn nonexistent_file_errors() {
        assert!(encrypt_zip(Path::new("/nonexistent/file.csv"), "pw").is_err());
    }
}
