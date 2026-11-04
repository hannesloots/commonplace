//! End-to-end behaviour of the commands agents use most, on a copy of
//! `fixtures/demo` with the clock pinned by `COMMONPLACE_NOW`.

mod common;

use common::{TODAY, demo_vault, run, run_json};

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
