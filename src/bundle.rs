// ═══════════════════════════════════════════════════════════
// Zephyr Bundle — self-contained executable support
// ═══════════════════════════════════════════════════════════
//
// How it works
// ────────────
// When `zephyr compile foo.zph` is run, this module creates a
// native executable by:
//
//   1. Copying the running `zephyr` binary verbatim
//   2. Appending the compiled .zphc bytecode
//   3. Appending an 8-byte magic sentinel  (ZPHPAYLD)
//   4. Appending an 8-byte LE u64 = length of bytecode
//
// Binary layout of the output file:
//
//   ┌──────────────────────────────────────┐
//   │  zephyr interpreter binary (intact)  │
//   │  ... all original bytes ...          │
//   ├──────────────────────────────────────┤
//   │  [N bytes]   .zphc bytecode          │
//   ├──────────────────────────────────────┤
//   │  [8 bytes]   sentinel: ZPHPAYLD      │
//   │  [8 bytes]   payload_len: u64 LE     │
//   └──────────────────────────────────────┘
//
// On startup (in main.rs, before CLI parsing), `extract_payload`
// reads the running binary's own tail. If sentinel matches, it
// slices out the bytecode bytes and returns the decoded AST.
// The interpreter then runs it directly — no files, no temp dirs.
//
// OS-specific output filename
// ───────────────────────────
//   Windows  →  <stem>.exe
//   macOS    →  <stem>          (no extension, chmod +x)
//   Linux    →  <stem>          (no extension, chmod +x)
//   Other    →  <stem>          (no extension, chmod +x attempt)
//
// ═══════════════════════════════════════════════════════════

use std::fs;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;
use crate::ast::Stmt;
use crate::bytecode;

// Magic sentinel — 8 ASCII bytes, unlikely to appear in normal binary data
const SENTINEL: &[u8; 8] = b"ZPHPAYLD";

// ── OS-aware output path ──────────────────────────────────────────────────

/// Return the platform-appropriate executable path for the given stem.
/// e.g. stem="hello"  →  "hello.exe" on Windows, "hello" elsewhere.
pub fn exe_path(stem: &str) -> String {
    if cfg!(target_os = "windows") {
        // Strip any existing .exe the user may have typed
        let clean = stem.strip_suffix(".exe").unwrap_or(stem);
        format!("{}.exe", clean)
    } else {
        stem.to_string()
    }
}

// ── Writing the executable ────────────────────────────────────────────────

/// Copy the running interpreter binary, append the bytecode payload,
/// set executable permissions (Unix), and return the output file size.
pub fn write_executable(bytecode_bytes: &[u8], output_path: &str) -> io::Result<u64> {
    // Find ourselves
    let self_path = std::env::current_exe()?;

    // Read the interpreter binary
    // On Windows, the running binary may be locked. We copy via read — this works
    // because we only need read access, not exclusive access.
    let interpreter_bytes = fs::read(&self_path)?;

    // Scrub any previous payload that may be on the interpreter binary itself.
    // This ensures `zephyr compile` on an already-compiled binary works correctly.
    let clean_interpreter = strip_payload(&interpreter_bytes);

    // Build the output in memory (avoids partial-write issues)
    let payload_len = bytecode_bytes.len() as u64;
    let mut out = Vec::with_capacity(clean_interpreter.len() + bytecode_bytes.len() + 16);
    out.extend_from_slice(clean_interpreter);
    out.extend_from_slice(bytecode_bytes);
    out.extend_from_slice(SENTINEL);
    out.extend_from_slice(&payload_len.to_le_bytes());

    let total = out.len() as u64;

    // Write atomically via temp file + rename (avoids corrupting output on error)
    let tmp_path = format!("{}.zph_tmp", output_path);
    {
        let mut f = fs::File::create(&tmp_path)?;
        f.write_all(&out)?;
        f.flush()?;
    }
    fs::rename(&tmp_path, output_path)?;

    // Make executable on Unix
    #[cfg(unix)]
    set_executable(output_path)?;

    Ok(total)
}

/// Strip any existing Zephyr payload from a binary.
/// Returns a slice of the original bytes up to the payload start.
fn strip_payload(data: &[u8]) -> &[u8] {
    if let Some(offset) = find_payload_start(data) {
        &data[..offset]
    } else {
        data
    }
}

/// Find the byte offset where our payload starts (i.e. right after the interpreter).
/// Returns None if no payload is present.
fn find_payload_start(data: &[u8]) -> Option<usize> {
    // Trailer is always: [N bytes payload][8 sentinel][8 length]
    if data.len() < 16 { return None; }

    let tail = &data[data.len() - 16..];
    if &tail[0..8] != SENTINEL { return None; }

    let payload_len = u64::from_le_bytes(tail[8..16].try_into().ok()?) as usize;
    let payload_start = data.len().checked_sub(16 + payload_len)?;
    Some(payload_start)
}

// ── Extracting the payload at runtime ────────────────────────────────────

/// Called at startup in main(). Reads the running binary's own bytes,
/// checks for a payload, and if found decodes and returns the AST.
/// Returns None if this binary has no embedded program (normal Zephyr CLI).
pub fn extract_payload() -> Option<Vec<Stmt>> {
    let self_path = std::env::current_exe().ok()?;
    let data = fs::read(&self_path).ok()?;

    if data.len() < 16 { return None; }

    let tail = &data[data.len() - 16..];
    if &tail[0..8] != SENTINEL { return None; }

    let payload_len = u64::from_le_bytes(tail[8..16].try_into().ok()?) as usize;
    let payload_start = data.len().checked_sub(16 + payload_len)?;
    let payload_end   = data.len() - 16;

    let bytecode_bytes = &data[payload_start..payload_end];

    match bytecode::decode(bytecode_bytes) {
        Ok((stmts, _hash)) => Some(stmts),
        Err(e) => {
            // Payload is present but corrupted — warn and fall through to CLI
            eprintln!("\x1b[33m[Zephyr warning]\x1b[0m Embedded payload is corrupt: {}", e);
            eprintln!("  Recompile with: zephyr compile <source.zph>");
            None
        }
    }
}

// ── Unix chmod helper ─────────────────────────────────────────────────────

#[cfg(unix)]
fn set_executable(path: &str) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let meta = fs::metadata(path)?;
    let mut perms = meta.permissions();
    // Add u+x g+x o+x (0o111) to existing permissions
    let mode = perms.mode() | 0o111;
    perms.set_mode(mode);
    fs::set_permissions(path, perms)
}

// On non-Unix platforms this is a no-op (Windows uses .exe extension)
#[cfg(not(unix))]
fn set_executable(_path: &str) -> io::Result<()> {
    Ok(())
}

// ═══════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roundtrip_payload_detection() {
        // Simulate: fake "interpreter binary" + appended payload
        let fake_interpreter = b"ELF\x7f_this_is_fake_interpreter_data_for_testing";
        let fake_bytecode = b"ZPHC\x01\x00some_bytecode_bytes_here";

        let payload_len = fake_bytecode.len() as u64;
        let mut bundle = Vec::new();
        bundle.extend_from_slice(fake_interpreter);
        bundle.extend_from_slice(fake_bytecode);
        bundle.extend_from_slice(SENTINEL);
        bundle.extend_from_slice(&payload_len.to_le_bytes());

        // Should find payload at correct offset
        let start = find_payload_start(&bundle).expect("should find payload");
        let end = bundle.len() - 16;
        assert_eq!(&bundle[start..end], fake_bytecode);

        // Stripping should return just the interpreter
        let stripped = strip_payload(&bundle);
        assert_eq!(stripped, fake_interpreter);
    }

    #[test]
    fn test_no_payload_returns_none() {
        let plain_binary = b"ELF\x7f_just_a_normal_binary_with_no_payload";
        assert!(find_payload_start(plain_binary).is_none());
    }

    #[test]
    fn test_strip_idempotent_on_clean_binary() {
        let plain = b"just some bytes";
        assert_eq!(strip_payload(plain), plain.as_ref());
    }

    #[test]
    fn test_exe_path_windows() {
        // Can't cfg-test the other branch, but at least test the logic
        // by directly calling the non-cfg parts
        #[cfg(target_os = "windows")]
        {
            assert_eq!(exe_path("hello"), "hello.exe");
            assert_eq!(exe_path("hello.exe"), "hello.exe"); // no double .exe
        }
        #[cfg(not(target_os = "windows"))]
        {
            assert_eq!(exe_path("hello"), "hello");
            assert_eq!(exe_path("my_program"), "my_program");
        }
    }
}