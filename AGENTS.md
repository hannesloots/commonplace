# commonplace — agent instructions (Rust code)

## What this is
A Rust CLI (`commonplace`) plus the Claude skill/plugin that ships it. `crates/commonplace-core`
is the library (all logic); `crates/commonplace-cli` is the thin clap front end. `skills/`,
`hooks/` and `fixtures/` are plugin content, not Rust. (`fixtures/demo/AGENTS.md` is a wiki's
instruction file, not this one.)

## Commands (run before declaring a task done)
- Format: `cargo fmt --all`
- Lint:   `cargo gate` (= clippy, all targets, warnings denied)
- Test:   `cargo test --workspace` and
          `cargo run -q -p commonplace-cli -- --root fixtures/demo lint --fail-on error`
- Docs:   `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`

Done = all four pass, no new `#[allow]`, every `#[expect]` has a `reason`.

## Rules
- Edition 2024, MSRV = `rust-version` in Cargo.toml (CI checks it). Add deps with `cargo add`.
- Errors: `anyhow::Result` with `.context()` throughout (the core crate is internal and only
  consumed by the CLI). No `unwrap`/`expect`/`panic!` outside tests; build regexes from literals
  with `crate::re`. Sort floats with `total_cmp`.
- Ownership: borrow by default. Every new `clone()`, `Arc`, `Box<dyn>` needs a reason.
- No `unsafe` (forbidden in Cargo.toml).
- Structure: logic lives in core; CLI command functions parse, call core, print. Items are
  private or `pub(crate)` unless the CLI needs them. Functions under ~80 lines.
- Output is a contract: the skill and hooks parse `--json` and the human output. Do not change
  either without updating `skills/commonplace` and CHANGELOG.md.
- Tests assert behaviour. Regressions get a test next to the fixed code.

## Workflow
Plan first, then small diffs. For refactors, record CLI output on a copy of `fixtures/demo`
before and after and diff it. To have format and clippy run after every edit, add
`scripts/dev/rust-check-hook.sh` as a `PostToolUse` hook (matcher `Edit|Write`) in
`.claude/settings.json`.
