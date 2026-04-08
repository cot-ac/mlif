//! Link an object file into a native executable via the system C compiler.

#![cfg(feature = "codegen")]

use std::process::Command;

/// Link an object file into a native executable by invoking `cc`.
///
/// The system C compiler handles platform details: linking against the
/// C runtime, setting the correct entry point (`_main` on macOS, `main`
/// on Linux), and producing a valid executable format.
pub fn link_executable(obj_path: &str, exe_path: &str) -> Result<(), String> {
    let status = Command::new("cc")
        .arg(obj_path)
        .arg("-o")
        .arg(exe_path)
        .status()
        .map_err(|e| format!("failed to run cc: {}", e))?;

    if !status.success() {
        return Err(format!(
            "linker failed with exit code: {}",
            status.code().map_or("signal".to_string(), |c| c.to_string())
        ));
    }

    Ok(())
}
