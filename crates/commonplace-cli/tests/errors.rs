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
fn unknown_walk_direction_is_a_usage_error() {
    let root = demo_vault();
    let out = run(
        &root,
        &[
            "--json",
            "graph",
            "walk",
            "Customer Onboarding",
            "--direction",
            "sideways",
        ],
    );
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(json_error(&out)["code"], "usage");
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

#[test]
fn yaml_syntax_error_in_frontmatter_names_the_page() {
    let root = demo_vault();
    std::fs::write(
        root.join("resources/concepts/Bad Yaml.md"),
        "---\ntitle: [unclosed\ntype: concept\n---\n\nBody.\n",
    )
    .unwrap();
    let findings = scratch_dir("review-yaml").join("findings.json");
    std::fs::write(&findings, "[]").unwrap();
    let out = run(
        &root,
        &[
            "--json",
            "review",
            "record",
            "skeptic",
            findings.to_str().unwrap(),
            "--scope",
            "Customer Onboarding,Bad Yaml",
        ],
    );
    let e = json_error(&out);
    assert_eq!(out.status.code(), Some(2), "{e}");
    assert_eq!(e["code"], "frontmatter", "{e}");
    assert!(
        e["path"]
            .as_str()
            .is_some_and(|p| p.ends_with("Bad Yaml.md")),
        "{e}"
    );
}

#[test]
fn lint_changed_without_git_survives_a_corrupt_manifest() {
    let root = demo_vault();
    std::fs::write(root.join(".commonplace/manifest.json"), "{ not json").unwrap();
    let out = run(&root, &["--json", "lint", "--changed", "--no-cite"]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        report["findings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["rule"] == "SRC-005"),
        "{report}"
    );
}
