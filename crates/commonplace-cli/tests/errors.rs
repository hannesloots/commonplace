//! The error contract agents rely on: exit code 2, `error[code]:` on stderr,
//! and a `{"error": {...}}` object on stdout with `--json`.

mod common;

use common::{demo_vault, json_error, run, scratch_dir};

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
    let empty = scratch_dir("empty");
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
