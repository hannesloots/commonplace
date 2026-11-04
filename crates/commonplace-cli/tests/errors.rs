//! The error contract agents rely on: exit code 2, `error[code]:` on stderr,
//! and a `{"error": {...}}` object on stdout with `--json`.
#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test helpers fail the test when setup fails"
)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

/// A fresh copy of `fixtures/demo` (commands such as `lint` write to the vault).
fn demo_vault() -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let dest = std::env::temp_dir().join(format!(
        "commonplace-cli-test-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dest);
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

fn run(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_commonplace"))
        .arg("--root")
        .arg(root)
        .args(args)
        .output()
        .unwrap()
}

fn json_error(out: &Output) -> serde_json::Value {
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "stdout is not JSON ({e}): {}",
            String::from_utf8_lossy(&out.stdout)
        )
    });
    v["error"].clone()
}

#[test]
fn missing_page_suggests_the_closest_title() {
    let root = demo_vault();
    let out = run(&root, &["--json", "graph", "walk", "Custmer Onboarding"]);
    assert_eq!(out.status.code(), Some(2));
    let e = json_error(&out);
    assert_eq!(e["code"], "page_not_found");
    assert_eq!(e["suggestions"][0], "Customer Onboarding");
    assert_eq!(e["hint"], "did you mean 'Customer Onboarding'?");
}

#[test]
fn human_error_names_the_code_and_hint_on_stderr() {
    let root = demo_vault();
    let out = run(&root, &["links", "backlinks", "Custmer Onboarding"]);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(String::from_utf8_lossy(&out.stdout), "");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.starts_with("error[page_not_found]: "), "{stderr}");
    assert!(
        stderr.contains("\n  hint: did you mean 'Customer Onboarding'?"),
        "{stderr}"
    );
}

#[test]
fn root_without_config_is_no_root() {
    let empty = std::env::temp_dir().join(format!("commonplace-cli-empty-{}", std::process::id()));
    std::fs::create_dir_all(&empty).unwrap();
    let out = run(&empty, &["--json", "status"]);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(json_error(&out)["code"], "no_root");
}

#[test]
fn malformed_query_is_invalid_query() {
    let root = demo_vault();
    let out = run(&root, &["--json", "query", "--where", "type=="]);
    assert_eq!(out.status.code(), Some(2));
    let e = json_error(&out);
    assert_eq!(e["code"], "invalid_query");
    assert!(
        e["hint"]
            .as_str()
            .is_some_and(|h| h.contains("field=value"))
    );
}

#[test]
fn unknown_flag_with_json_is_a_usage_error_object() {
    let root = demo_vault();
    let out = run(&root, &["--json", "status", "--bogus"]);
    assert_eq!(out.status.code(), Some(2));
    let e = json_error(&out);
    assert_eq!(e["code"], "usage");
    assert!(e["message"].as_str().is_some_and(|m| m.contains("--bogus")));
}

#[test]
fn unknown_source_is_source_not_found() {
    let root = demo_vault();
    let out = run(&root, &["--json", "source", "show", "no-such-source"]);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(json_error(&out)["code"], "source_not_found");
}

#[test]
fn demo_vault_lints_clean() {
    let root = demo_vault();
    let out = run(&root, &["lint", "--fail-on", "error"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
}
