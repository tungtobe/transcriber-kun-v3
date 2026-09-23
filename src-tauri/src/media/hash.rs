use std::{fs::File, io::Read, path::Path};

use sha2::{Digest, Sha256};

use super::storage_error;

const HASH_BUFFER_BYTES: usize = 64 * 1024;

/// Hashes a source file without loading it into memory.
pub fn sha256_file(path: &Path) -> Result<String, crate::core::error::AppError> {
    let mut file = File::open(path).map_err(|_| storage_error())?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; HASH_BUFFER_BYTES];

    loop {
        let read = file.read(&mut buffer).map_err(|_| storage_error())?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }

    let hash = digest.finalize();
    let mut hex = String::with_capacity(hash.len() * 2);
    for byte in hash {
        use std::fmt::Write as _;
        let _ = write!(hex, "{byte:02x}");
    }
    Ok(hex)
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    use super::*;

    #[test]
    fn sha256_file_streams_and_returns_lowercase_hex() {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        file.write_all(b"abc").unwrap();

        assert_eq!(
            sha256_file(file.path()).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
