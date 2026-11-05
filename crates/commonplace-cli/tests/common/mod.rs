//! Helpers shared by the CLI integration tests: a scratch copy of
//! `fixtures/demo` and a runner with a pinned clock.
#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test helpers fail the test when setup fails"
)]
#![allow(
    dead_code,
    reason = "each test binary uses a different subset of these helpers"
)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

/// The date every test run sees through `COMMONPLACE_NOW`.
pub(crate) const TODAY: &str = "2026-03-14";

/// A fresh scratch directory under the system temp dir.
pub(crate) fn scratch_dir(label: &str) -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "commonplace-cli-{label}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A fresh copy of `fixtures/demo` (commands such as `lint` write to the vault).
pub(crate) fn demo_vault() -> PathBuf {
    let dest = scratch_dir("demo");
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/demo");
    copy_dir(&src, &dest);
    dest
}

fn copy_dir(src: &Path, dest: &Path) {
    std::fs::create_dir_all(dest).unwrap();
    for entry in std::fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let to = dest.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &to);
        } else {
            std::fs::copy(entry.path(), &to).unwrap();
        }
    }
}

/// Run `commonplace --root <root> <args>` with the clock pinned to [`TODAY`].
pub(crate) fn run(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_commonplace"))
        .env("COMMONPLACE_NOW", TODAY)
        .arg("--root")
        .arg(root)
        .args(args)
        .output()
        .unwrap()
}

/// Run with `--json` and parse stdout, asserting the expected exit code.
pub(crate) fn run_json(root: &Path, args: &[&str], exit: i32) -> serde_json::Value {
    let mut all = vec!["--json"];
    all.extend_from_slice(args);
    let out = run(root, &all);
    assert_eq!(
        out.status.code(),
        Some(exit),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    parse_json(&out)
}

/// Parse stdout as JSON, failing the test with the raw text if it is not.
pub(crate) fn parse_json(out: &Output) -> serde_json::Value {
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "stdout is not JSON ({e}): {}",
            String::from_utf8_lossy(&out.stdout)
        )
    })
}

/// The `error` object of a `--json` failure.
pub(crate) fn json_error(out: &Output) -> serde_json::Value {
    parse_json(out)["error"].clone()
}
