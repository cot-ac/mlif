//! Write Cranelift object file bytes to disk.

#![cfg(feature = "codegen")]

use std::fs;
use std::path::Path;

/// Write raw object file bytes to the given path.
pub fn write_object_file(bytes: &[u8], path: &str) -> Result<(), String> {
    let path = Path::new(path);

    // Ensure the parent directory exists.
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create directory {}: {}", parent.display(), e))?;
        }
    }

    fs::write(path, bytes).map_err(|e| format!("failed to write object file {}: {}", path.display(), e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_and_read_back() {
        let tmp = std::env::temp_dir().join("mlif_emit_test.o");
        let path = tmp.to_str().unwrap();
        let data = b"\x7fELF_fake_object";

        write_object_file(data, path).unwrap();

        let read_back = fs::read(&tmp).unwrap();
        assert_eq!(read_back, data);

        // Clean up.
        let _ = fs::remove_file(&tmp);
    }
}
