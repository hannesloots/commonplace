//! End-to-end behaviour of the commands agents use most, on a copy of
//! `fixtures/demo` with the clock pinned by `COMMONPLACE_NOW`.

mod common;

use common::{TODAY, demo_vault, run, run_json, scratch_dir};
use serde_json::Value;

fn strings(v: &Value) -> Vec<&str> {
    v.as_array()
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

#[test]
fn lint_json_counts_agree_with_its_findings() {
    let root = demo_vault();
    let report = run_json(&root, &["lint"], 0);
    let findings = report["findings"].as_array().unwrap();
    for (key, severity) in [
        ("errors", "error"),
        ("warnings", "warning"),
        ("infos", "info"),
    ] {
        let counted = findings
            .iter()
            .filter(|f| f["severity"] == severity)
            .count();
        assert_eq!(report[key].as_u64(), Some(counted as u64), "{key}");
    }
    assert_eq!(report["errors"], 0);
    assert!(report["pages_checked"].as_u64().is_some_and(|n| n > 0));
}

#[test]
fn every_demo_citation_verifies() {
    let root = demo_vault();
    let results = run_json(&root, &["cite", "verify"], 0);
    let results = results.as_array().unwrap();
    assert_ne!(results.len(), 0, "no citations in the demo");
    for r in results {
        assert_eq!(r["verdict"], "pass", "{r}");
    }
}

#[test]
fn search_ranks_the_matching_capability_first() {
    let root = demo_vault();
    let hits = run_json(&root, &["search", "customer onboarding"], 0);
    assert_eq!(hits[0]["title"], "Customer Onboarding");
    let scores: Vec<f64> = hits
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|h| h["score"].as_f64())
        .collect();
    assert!(scores.windows(2).all(|w| w[0] >= w[1]), "{scores:?}");
}

#[test]
fn ingesting_markdown_registers_a_source_with_note_and_text_layer() {
    let root = demo_vault();
    let file = scratch_dir("ingest").join("field.md");
    std::fs::write(
        &file,
        "# Field notes\n\nThe onboarding team met on Tuesday.\n",
    )
    .unwrap();
    let file = file.to_str().unwrap();
    let made = run_json(
        &root,
        &[
            "ingest",
            file,
            "--title",
            "Field Notes",
            "--date",
            "2026-01-02",
        ],
        0,
    );
    let id = made[0]["id"].as_str().unwrap();
    for key in ["raw", "text", "note"] {
        let rel = made[0][key].as_str().unwrap();
        assert!(root.join(rel).is_file(), "{key}: {rel}");
    }
    let listed = run_json(&root, &["source", "list"], 0);
    assert!(
        listed.as_array().unwrap().iter().any(|s| s["id"] == id),
        "{listed}"
    );
    let text = std::fs::read_to_string(root.join(made[0]["text"].as_str().unwrap())).unwrap();
    assert!(text.contains("The onboarding team met on Tuesday."));
}

#[test]
fn rename_moves_every_link_to_the_new_title() {
    let root = demo_vault();
    let before = run_json(&root, &["links", "backlinks", "Open an account"], 0);
    assert_ne!(strings(&before).len(), 0, "the demo page has backlinks");
    assert!(
        run(&root, &["rename", "Open an account", "Open a bank account"])
            .status
            .success()
    );
    assert_eq!(
        run_json(&root, &["links", "check"], 0),
        Value::Array(vec![])
    );
    let after = run_json(&root, &["links", "backlinks", "Open a bank account"], 0);
    assert_eq!(strings(&after), strings(&before));
}

#[test]
fn export_keeps_out_of_order_footnotes_distinct() {
    let root = demo_vault();
    std::fs::write(
        root.join("resources/concepts/Footnote Order.md"),
        "---\ntitle: Footnote Order\ntype: concept\n---\n# Footnote Order\n\n\
         Alpha[^2] then beta[^1].\n\n[^2]: second\n[^1]: first\n",
    )
    .unwrap();
    let out = run(&root, &["export", "Footnote Order"]);
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("Alpha[^1] then beta[^2]."), "{text}");
    assert!(
        text.contains("[^1]: second") && text.contains("[^2]: first"),
        "{text}"
    );
}

#[test]
fn pinned_clock_dates_new_pages_and_log_entries() {
    let root = demo_vault();
    let made = run_json(&root, &["new", "concept", "Pinned Clock"], 0);
    let page = std::fs::read_to_string(root.join(made["path"].as_str().unwrap())).unwrap();
    assert!(page.contains(&format!("created: {TODAY}\n")), "{page}");
    assert!(
        run(&root, &["log", "add", "note", "Clock check"])
            .status
            .success()
    );
    let log = std::fs::read_to_string(root.join("log.md")).unwrap();
    assert!(
        log.contains(&format!("## [{TODAY}] note | Clock check")),
        "{log}"
    );
}

#[test]
fn pack_rule_with_invalid_regex_is_reported_once() {
    let root = demo_vault();
    let pack = root.join(".commonplace/packs/legal/pack.yaml");
    let yaml = std::fs::read_to_string(&pack).unwrap();
    let broken = yaml.replace(
        r"pattern: '\[\^[A-Za-z0-9_\-]+\]'",
        r"pattern: '\[\^(unclosed'",
    );
    assert_ne!(
        broken, yaml,
        "fixture rule LEG-005 changed; update this test"
    );
    std::fs::write(&pack, broken).unwrap();
    let report = run_json(&root, &["lint", "--rule", "PACK"], 1);
    let findings = report["findings"].as_array().unwrap();
    assert_eq!(findings.len(), 1, "{report}");
    let f = &findings[0];
    assert_eq!(f["rule"], "PACK-001");
    assert_eq!(f["severity"], "error");
    assert_eq!(f["path"], ".commonplace/packs/legal/pack.yaml");
    assert!(f["message"].as_str().unwrap().contains("LEG-005"), "{f}");
}

#[test]
fn corrupt_manifest_is_a_finding_and_is_left_untouched() {
    let root = demo_vault();
    let manifest = root.join(".commonplace/manifest.json");
    std::fs::write(&manifest, "{ not json").unwrap();
    let report = run_json(&root, &["lint", "--rule", "SRC"], 1);
    let findings = report["findings"].as_array().unwrap();
    let f = findings
        .iter()
        .find(|f| f["rule"] == "SRC-005")
        .unwrap_or_else(|| panic!("no SRC-005 in {report}"));
    assert_eq!(f["path"], ".commonplace/manifest.json");
    assert!(
        findings.iter().all(|f| f["rule"] != "SRC-003"),
        "raw files must not all be flagged as unregistered: {report}"
    );
    assert_eq!(std::fs::read_to_string(&manifest).unwrap(), "{ not json");
}

#[test]
fn review_record_writes_nothing_when_a_page_in_scope_is_broken() {
    let root = demo_vault();
    let good = root.join("projects/demo-engagement/architecture/business/Customer Onboarding.md");
    let before = std::fs::read_to_string(&good).unwrap();
    std::fs::write(
        root.join("resources/concepts/Broken Page.md"),
        "---\ntitle: Broken Page\n# never closed\n",
    )
    .unwrap();
    let findings = scratch_dir("review").join("findings.json");
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
            "Customer Onboarding,Broken Page",
        ],
    );
    assert_eq!(out.status.code(), Some(2));
    let e = common::json_error(&out);
    assert_eq!(e["code"], "frontmatter", "{e}");
    assert!(
        e["message"].as_str().unwrap().contains("Broken Page"),
        "{e}"
    );
    assert_eq!(std::fs::read_to_string(&good).unwrap(), before);
    let reviews: Vec<_> = std::fs::read_dir(root.join("reviews"))
        .map(|d| d.flatten().map(|e| e.file_name()).collect())
        .unwrap_or_default();
    assert!(
        reviews
            .iter()
            .all(|n| !n.to_string_lossy().contains("skeptic")),
        "{reviews:?}"
    );
}

#[test]
fn walk_direction_selects_outgoing_or_incoming_links() {
    let root = demo_vault();
    let nodes = |direction: &str| -> Vec<String> {
        let rows = run_json(
            &root,
            &[
                "graph",
                "walk",
                "Customer Onboarding",
                "--depth",
                "1",
                "--direction",
                direction,
            ],
            0,
        );
        rows.as_array()
            .unwrap()
            .iter()
            .filter_map(|r| r["node"].as_str().map(String::from))
            .collect()
    };
    let (out, inn, both) = (nodes("out"), nodes("in"), nodes("both"));
    // Customer Onboarding is realised_by the workflow service, and the
    // "Open an account" use case is enabled_by it.
    assert!(
        out.contains(&"Onboarding Workflow Service".to_string()),
        "{out:?}"
    );
    assert!(!out.contains(&"Open an account".to_string()), "{out:?}");
    assert!(inn.contains(&"Open an account".to_string()), "{inn:?}");
    assert!(out.iter().chain(&inn).all(|n| both.contains(n)), "{both:?}");
}

#[test]
fn query_compares_two_fields_with_or_without_backticks() {
    let root = demo_vault();
    for expr in [
        "type=capability and target_maturity>baseline_maturity",
        "type=capability and `target_maturity>baseline_maturity`",
    ] {
        let rows = run_json(&root, &["query", "--where", expr], 0);
        assert!(
            rows.to_string().contains("Customer Onboarding"),
            "{expr}: {rows}"
        );
    }
}

#[test]
fn lint_changed_falls_back_to_the_last_commit_when_only_non_markdown_is_dirty() {
    let root = demo_vault();
    let git = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(&root)
            .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
            .args(["-c", "commit.gpgsign=false"])
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    };
    git(&["init", "-q"]);
    git(&["add", "-A"]);
    git(&["commit", "-q", "-m", "base"]);
    let stub =
        "projects/demo-engagement/architecture/building-blocks/Onboarding Workflow Service.md";
    let mut text = std::fs::read_to_string(root.join(stub)).unwrap();
    text.push_str("\n<!-- touched -->\n");
    std::fs::write(root.join(stub), text).unwrap();
    git(&["add", "-A"]);
    git(&["commit", "-q", "-m", "touch one stub"]);
    // A plain lint rewrites the manifest, so the tree is dirty but no page is.
    run(&root, &["lint"]);

    let report = run_json(&root, &["lint", "--changed"], 0);
    let paths: Vec<&str> = report["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|f| f["path"].as_str())
        .collect();
    assert!(
        paths.contains(&stub),
        "last commit's page not linted: {report}"
    );
    assert!(
        !paths.iter().any(|p| p.ends_with("Customer Onboarding.md")),
        "an untouched page was linted: {report}"
    );
}
