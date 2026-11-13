//! The skills layer end to end: propose, gate, install, record use, retire,
//! on a copy of `fixtures/demo` with the clock pinned by `COMMONPLACE_NOW`.
#![expect(
    clippy::unwrap_used,
    reason = "test helpers fail the test when setup fails"
)]

mod common;

use common::{TODAY, demo_vault, json_error, parse_json, run, run_json, scratch_dir};
use serde_json::Value;
use std::path::Path;
use std::process::{Command, Output};

const DESC: &str = "Prepare the weekly review. Use when the user asks for a weekly review.";

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

/// Lint findings whose rule starts with `SKILL` (lint exits 1 when one is an error).
fn skill_rules(root: &Path) -> Vec<String> {
    let out = run(root, &["--json", "lint", "--rule", "SKILL"]);
    assert!(matches!(out.status.code(), Some(0 | 1)), "{out:?}");
    parse_json(&out)["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|f| f["rule"].as_str().map(str::to_string))
        .collect()
}

/// A demo vault with a new skill `weekly-review` whose PURPOSE.md links a real page.
fn vault_with_skill() -> std::path::PathBuf {
    let root = demo_vault();
    run_json(
        &root,
        &["skill", "new", "weekly-review", "--description", DESC],
        0,
    );
    let purpose = root.join("skills/weekly-review/PURPOSE.md");
    let text = read(&purpose).replace(
        "## Motivated by\n",
        "## Motivated by\n\n- [[Customer Onboarding]]: the steps\n",
    );
    std::fs::write(&purpose, text).unwrap();
    root
}

fn accept(root: &Path, reason: &str) -> Value {
    run_json(
        root,
        &["skill", "accept", "weekly-review", "--reason", reason],
        0,
    )
}

/// `weekly-review` in `skill list` (the demo vault has a skill of its own), or null.
fn list_entry(root: &Path) -> Value {
    run_json(root, &["skill", "list"], 0)
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "weekly-review")
        .cloned()
        .unwrap_or(Value::Null)
}

#[test]
fn a_new_skill_is_a_proposal_until_accepted_then_installed_without_purpose() {
    let root = vault_with_skill();
    let skill_md = read(&root.join("skills/weekly-review/SKILL.md"));
    assert!(skill_md.contains("name: weekly-review"), "{skill_md}");
    assert!(skill_md.contains("# Weekly review"), "{skill_md}");
    assert_eq!(list_entry(&root)["state"], "new");
    assert!(skill_rules(&root).contains(&"SKILL-006".to_string()));
    assert!(!root.join(".claude/skills/weekly-review").exists());

    let rep = run_json(
        &root,
        &[
            "skill",
            "accept",
            "weekly-review",
            "--reason",
            "first version",
            "--evidence",
            "Customer Onboarding",
        ],
        0,
    );
    assert_eq!(rep["decision"], "accepted");
    for target in [".claude/skills", ".agents/skills"] {
        let dir = root.join(target).join("weekly-review");
        assert_eq!(read(&dir.join("SKILL.md")), skill_md);
        assert!(
            !dir.join("PURPOSE.md").exists(),
            "PURPOSE.md is for the wiki"
        );
    }
    let impact = read(&root.join("skill-impact.md"));
    assert!(
        impact.contains(&format!("## [{TODAY}] accepted | weekly-review")),
        "{impact}"
    );
    assert!(
        impact.contains("- evidence: [[Customer Onboarding]]"),
        "{impact}"
    );
    assert!(impact.contains("+++ b/SKILL.md"), "{impact}");
    assert_eq!(list_entry(&root)["state"], "accepted");
    assert_eq!(skill_rules(&root), Vec::<String>::new());
}

#[test]
fn rejecting_a_change_restores_the_accepted_version_and_keeps_the_record() {
    let root = vault_with_skill();
    accept(&root, "first version");
    let path = root.join("skills/weekly-review/SKILL.md");
    let accepted = read(&path);
    std::fs::write(
        &path,
        accepted.replace("## Steps", "## Steps\n\n1. Skip the inbox"),
    )
    .unwrap();
    assert_eq!(list_entry(&root)["state"], "changed");
    let d = run_json(&root, &["skill", "diff", "weekly-review"], 0);
    assert!(
        d["diff"].as_str().unwrap().contains("+1. Skip the inbox"),
        "{d}"
    );

    run_json(
        &root,
        &[
            "skill",
            "reject",
            "weekly-review",
            "--reason",
            "inbox matters",
        ],
        0,
    );
    assert_eq!(read(&path), accepted);
    let impact = read(&root.join("skill-impact.md"));
    assert!(impact.contains("rejected | weekly-review"), "{impact}");
    assert!(
        impact.contains("+1. Skip the inbox"),
        "the rejected idea stays on record"
    );
}

#[test]
fn rejecting_a_new_skill_removes_its_folder() {
    let root = vault_with_skill();
    run_json(
        &root,
        &["skill", "reject", "weekly-review", "--reason", "not needed"],
        0,
    );
    assert!(!root.join("skills/weekly-review").exists());
    assert!(read(&root.join("skill-impact.md")).contains("rejected | weekly-review"));
}

#[test]
fn uses_are_counted_from_the_impact_record() {
    let root = vault_with_skill();
    accept(&root, "first version");
    run_json(
        &root,
        &["skill", "used", "weekly-review", "--outcome", "ok"],
        0,
    );
    run_json(
        &root,
        &[
            "skill",
            "used",
            "weekly-review",
            "--outcome",
            "failed",
            "--note",
            "missed a project",
        ],
        0,
    );
    let s = list_entry(&root);
    assert_eq!(
        (s["uses"].as_u64(), s["failures"].as_u64()),
        (Some(2), Some(1))
    );
    assert_eq!(s["last_used"], TODAY);
}

#[test]
fn deleting_the_source_and_accepting_retires_the_skill() {
    let root = vault_with_skill();
    accept(&root, "first version");
    std::fs::remove_dir_all(root.join("skills/weekly-review")).unwrap();
    assert_eq!(list_entry(&root)["state"], "removed");
    accept(&root, "replaced by the monthly review");
    assert!(!root.join(".claude/skills/weekly-review").exists());
    assert_eq!(list_entry(&root), Value::Null);
}

#[test]
fn lint_flags_wiki_links_in_skills_and_unresolved_purpose_links() {
    let root = vault_with_skill();
    let dir = root.join("skills/weekly-review");
    let skill_md = read(&dir.join("SKILL.md"));
    std::fs::write(
        dir.join("SKILL.md"),
        format!("{skill_md}\nSee [[Customer Onboarding]].\n"),
    )
    .unwrap();
    let purpose =
        read(&dir.join("PURPOSE.md")).replace("[[Customer Onboarding]]", "[[No Such Page]]");
    std::fs::write(dir.join("PURPOSE.md"), purpose).unwrap();
    assert!(skill_rules(&root).contains(&"SKILL-005".to_string()));
    // PURPOSE.md is a page, so its broken link is an ordinary broken link
    let report = run_json(&root, &["lint", "--rule", "LINK-001"], 1);
    assert_eq!(
        report["findings"][0]["path"],
        "skills/weekly-review/PURPOSE.md"
    );
}

#[test]
fn a_skill_is_a_page_in_the_graph_and_the_index() {
    let root = vault_with_skill();
    let back = run_json(&root, &["links", "backlinks", "Customer Onboarding"], 0);
    assert!(
        back.as_array()
            .unwrap()
            .iter()
            .any(|b| b == "skills/weekly-review/PURPOSE.md"),
        "{back}"
    );
    run_json(&root, &["index"], 0);
    let index = read(&root.join("index.md"));
    assert!(index.contains("## Skills"), "{index}");
    assert!(
        index.contains(&format!("[[weekly-review]] — {DESC}")),
        "{index}"
    );
}

#[test]
fn renaming_a_linked_page_rewrites_purpose_without_making_a_proposal() {
    let root = vault_with_skill();
    accept(&root, "first version");
    run_json(
        &root,
        &["rename", "Customer Onboarding", "Client Onboarding"],
        0,
    );
    for purpose in [
        root.join("skills/weekly-review/PURPOSE.md"),
        root.join(".commonplace/skills/weekly-review/PURPOSE.md"),
    ] {
        assert!(
            read(&purpose).contains("[[Client Onboarding]]"),
            "{}",
            purpose.display()
        );
    }
    assert_eq!(list_entry(&root)["state"], "accepted");
    let out = run(&root, &["--json", "rename", "weekly-review", "x"]);
    assert_eq!(json_error(&out)["code"], "invalid_input");
}

#[test]
fn installed_copy_edited_by_hand_is_reported_and_sync_restores_it() {
    let root = vault_with_skill();
    accept(&root, "first version");
    let installed = root.join(".claude/skills/weekly-review/SKILL.md");
    std::fs::write(&installed, "edited\n").unwrap();
    assert!(skill_rules(&root).contains(&"SKILL-007".to_string()));
    run_json(&root, &["skill", "sync"], 0);
    assert_eq!(
        read(&installed),
        read(&root.join("skills/weekly-review/SKILL.md"))
    );
}

/// Run with `HOME` (and `USERPROFILE`) pointing at `home`.
fn run_with_home(root: &Path, home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_commonplace"))
        .env("COMMONPLACE_NOW", TODAY)
        .env("HOME", home)
        .env("USERPROFILE", home)
        .arg("--root")
        .arg(root)
        .arg("--json")
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn user_sync_installs_under_home_and_never_overwrites_a_foreign_skill() {
    let root = vault_with_skill();
    accept(&root, "first version");
    let home = scratch_dir("home");
    let out = run_with_home(&root, &home, &["skill", "sync", "--user"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(home.join(".claude/skills/weekly-review/SKILL.md").is_file());
    assert!(read(&root.join("commonplace.toml")).contains("weekly-review"));

    let foreign = scratch_dir("home");
    std::fs::create_dir_all(foreign.join(".claude/skills/weekly-review")).unwrap();
    std::fs::write(
        foreign.join(".claude/skills/weekly-review/SKILL.md"),
        "mine\n",
    )
    .unwrap();
    let out = run_with_home(&root, &foreign, &["skill", "sync", "--user"]);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(json_error(&out)["code"], "already_exists");
    assert_eq!(
        read(&foreign.join(".claude/skills/weekly-review/SKILL.md")),
        "mine\n"
    );
}

#[test]
fn skill_errors_carry_stable_codes() {
    let root = demo_vault();
    let code = |args: &[&str]| {
        let mut all = vec!["--json"];
        all.extend_from_slice(args);
        let out = run(&root, &all);
        assert_eq!(
            out.status.code(),
            Some(2),
            "{}",
            String::from_utf8_lossy(&out.stdout)
        );
        json_error(&out)["code"].as_str().unwrap().to_string()
    };
    assert_eq!(code(&["skill", "diff", "nope"]), "skill_not_found");
    assert_eq!(
        code(&["skill", "used", "nope", "--outcome", "ok"]),
        "skill_not_found"
    );
    assert_eq!(
        code(&["skill", "new", "Bad_Name", "--description", DESC]),
        "invalid_input"
    );
    run_json(
        &root,
        &["skill", "new", "weekly-review", "--description", DESC],
        0,
    );
    assert_eq!(
        code(&["skill", "new", "weekly-review", "--description", DESC]),
        "already_exists"
    );
    assert_eq!(
        code(&[
            "skill",
            "accept",
            "weekly-review",
            "--reason",
            "x",
            "--evidence",
            "Custmer Onboarding"
        ]),
        "page_not_found"
    );
    run_json(
        &root,
        &["skill", "accept", "weekly-review", "--reason", "first"],
        0,
    );
    assert_eq!(
        code(&["skill", "accept", "weekly-review", "--reason", "again"]),
        "invalid_input"
    );
}
