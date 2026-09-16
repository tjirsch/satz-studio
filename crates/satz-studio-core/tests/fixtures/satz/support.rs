//! Shared by the `satz_binary` and `satz_init` tests: a fake `satz` script, proven
//! runnable before it is handed to the code under test. Each test file includes it with
//! `#[path = "fixtures/satz/support.rs"]`.
//!
//! The fakes are a unix fixture — a shell script with a shebang — so each test file
//! declares the module `#[cfg(unix)]` and the tests that use it are `#[cfg(unix)]` too.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::time::Duration;

/// A script named `satz` in `dir` whose body is `body`, proven runnable before it is
/// returned.
///
/// The proof is not ceremony. These tests run on threads of one process, and a thread
/// that still holds a write handle to a file another thread is executing makes that
/// exec fail with `ETXTBSY`. The window is short and the failure is a flake, so the
/// helper runs the script itself until it starts, and only then returns.
pub fn fake(dir: &Path, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join("satz");
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        match std::process::Command::new(&path).arg("--version").output() {
            // It ran: whatever it printed or exited with is the test's business.
            Ok(_) => return path,
            Err(e)
                if e.raw_os_error() == Some(libc_etxtbsy())
                    && std::time::Instant::now() < deadline =>
            {
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(e) => panic!("{} is not runnable: {e}", path.display()),
        }
    }
}

/// `ETXTBSY`, the errno for executing a file somebody is still writing.
fn libc_etxtbsy() -> i32 {
    26
}
