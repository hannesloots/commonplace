//! The error type returned by every fallible function in this crate.
//!
//! An [`Error`] carries a stable machine-readable [`code`](Error::code), a
//! human message (its `Display`), an optional [`hint`](Error::hint) saying
//! what to do next, and, for missing pages, close-match
//! [`suggestions`](Error::suggestions). The CLI prints these as
//! `error[code]: message` or, with `--json`, as an object, so an agent can
//! branch on the code instead of parsing prose.

use std::backtrace::Backtrace;
use std::fmt;
use std::path::{Path, PathBuf};

/// `Result` with this crate's [`Error`].
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Error returned by commonplace-core: a stable [`code`](Error::code), a
/// message (`Display`), an optional [`hint`](Error::hint) and, for missing
/// pages, [`suggestions`](Error::suggestions).
pub struct Error {
    kind: Kind,
    backtrace: Backtrace,
}

type BoxError = Box<dyn std::error::Error + Send + Sync>;

#[derive(Debug, thiserror::Error)]
enum Kind {
    #[error("no page named '{name}'")]
    PageNotFound {
        name: String,
        suggestions: Vec<String>,
    },
    #[error("unknown source id '{id}'")]
    SourceNotFound { id: String },
    #[error("no skill named '{name}'")]
    SkillNotFound { name: String },
    #[error("{message}")]
    TextLayerMissing { message: String },
    #[error("{what} already exists")]
    AlreadyExists {
        what: String,
        advice: Option<&'static str>,
    },
    #[error("no commonplace.toml found in {} or its parents", start.display())]
    NoRoot { start: PathBuf },
    #[error("{message}")]
    InvalidQuery { message: String },
    #[error("{message}")]
    InvalidInput { message: String },
    #[error("{}{message}", path_prefix(path.as_deref()))]
    Frontmatter {
        path: Option<PathBuf>,
        message: String,
    },
    #[error("{message}")]
    Conversion { message: String },
    #[error("{op} {}: {source}", path.display())]
    Io {
        op: &'static str,
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{source}")]
    IoUnlocated { source: std::io::Error },
    #[error("parsing {}: {source}", path.display())]
    Parse { path: PathBuf, source: BoxError },
    #[error("{source}")]
    ParseUnlocated { source: BoxError },
}

fn path_prefix(path: Option<&Path>) -> String {
    path.map(|p| format!("{}: ", p.display()))
        .unwrap_or_default()
}

impl Error {
    fn new(kind: Kind) -> Self {
        Self {
            kind,
            backtrace: Backtrace::capture(),
        }
    }

    /// No page matches `name`; `suggestions` are close titles, best first.
    pub fn page_not_found(name: impl Into<String>, suggestions: Vec<String>) -> Self {
        Self::new(Kind::PageNotFound {
            name: name.into(),
            suggestions,
        })
    }

    /// No source is registered under `id`.
    pub fn source_not_found(id: impl Into<String>) -> Self {
        Self::new(Kind::SourceNotFound { id: id.into() })
    }

    /// No skill folder `skills/<name>/` (or accepted copy) exists.
    pub fn skill_not_found(name: impl Into<String>) -> Self {
        Self::new(Kind::SkillNotFound { name: name.into() })
    }

    /// A source has no text layer (or the requested span is not in it).
    pub fn text_layer_missing(message: impl Into<String>) -> Self {
        Self::new(Kind::TextLayerMissing {
            message: message.into(),
        })
    }

    /// The page, file or folder described by `what` already exists.
    pub fn already_exists(what: impl Into<String>) -> Self {
        Self::new(Kind::AlreadyExists {
            what: what.into(),
            advice: None,
        })
    }

    /// Like [`already_exists`](Self::already_exists), with `advice` as the hint.
    pub fn already_exists_with(what: impl Into<String>, advice: &'static str) -> Self {
        Self::new(Kind::AlreadyExists {
            what: what.into(),
            advice: Some(advice),
        })
    }

    /// No `commonplace.toml` was found walking up from `start`.
    pub fn no_root(start: &Path) -> Self {
        Self::new(Kind::NoRoot {
            start: start.to_path_buf(),
        })
    }

    /// A `query --where` expression could not be parsed.
    pub fn invalid_query(message: impl Into<String>) -> Self {
        Self::new(Kind::InvalidQuery {
            message: message.into(),
        })
    }

    /// An argument or input file is unusable (bad chain, missing title, wrong format, ...).
    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self::new(Kind::InvalidInput {
            message: message.into(),
        })
    }

    /// A page's YAML frontmatter is malformed.
    pub fn frontmatter(path: Option<&Path>, message: impl Into<String>) -> Self {
        Self::new(Kind::Frontmatter {
            path: path.map(Path::to_path_buf),
            message: message.into(),
        })
    }

    /// Converting a document to markdown failed.
    pub fn conversion(message: impl Into<String>) -> Self {
        Self::new(Kind::Conversion {
            message: message.into(),
        })
    }

    /// An I/O operation (`op`, e.g. "reading") on `path` failed.
    pub fn io(op: &'static str, path: &Path, source: std::io::Error) -> Self {
        Self::new(Kind::Io {
            op,
            path: path.to_path_buf(),
            source,
        })
    }

    /// Parsing the file at `path` (YAML, JSON or TOML) failed.
    pub fn parse(path: &Path, source: impl Into<BoxError>) -> Self {
        Self::new(Kind::Parse {
            path: path.to_path_buf(),
            source: source.into(),
        })
    }

    /// Attach `path` to a frontmatter error that was raised without one.
    #[must_use]
    pub fn in_file(mut self, file: &Path) -> Self {
        if let Kind::Frontmatter { path: p @ None, .. } = &mut self.kind {
            *p = Some(file.to_path_buf());
        }
        self
    }

    /// Stable machine-readable code, e.g. `page_not_found`. Documented in the
    /// skill's `references/cli.md`; never change an existing code.
    pub fn code(&self) -> &'static str {
        match &self.kind {
            Kind::PageNotFound { .. } => "page_not_found",
            Kind::SourceNotFound { .. } => "source_not_found",
            Kind::SkillNotFound { .. } => "skill_not_found",
            Kind::TextLayerMissing { .. } => "text_layer_missing",
            Kind::AlreadyExists { .. } => "already_exists",
            Kind::NoRoot { .. } => "no_root",
            Kind::InvalidQuery { .. } => "invalid_query",
            Kind::InvalidInput { .. } => "invalid_input",
            Kind::Frontmatter { .. } => "frontmatter",
            Kind::Conversion { .. } => "conversion",
            Kind::Io { .. } | Kind::IoUnlocated { .. } => "io",
            Kind::Parse { .. } | Kind::ParseUnlocated { .. } => "parse",
        }
    }

    /// What the caller should try next, if there is a useful general answer.
    pub fn hint(&self) -> Option<String> {
        match &self.kind {
            Kind::PageNotFound { suggestions, .. } => Some(match suggestions.first() {
                Some(best) => format!("did you mean '{best}'?"),
                None => "find the title with `commonplace search <words>`".into(),
            }),
            Kind::SourceNotFound { .. } => Some("list ids with `commonplace source list`".into()),
            Kind::SkillNotFound { .. } => Some(
                "list skills with `commonplace skill list`, or create one with `commonplace skill new`"
                    .into(),
            ),
            Kind::TextLayerMissing { .. } => Some(
                "convert the original, then register it with `commonplace source attach-text`"
                    .into(),
            ),
            Kind::AlreadyExists { advice, .. } => Some(
                advice
                    .unwrap_or("choose another name, or edit the existing one")
                    .into(),
            ),
            Kind::NoRoot { .. } => Some("run `commonplace init <dir>` or pass --root <dir>".into()),
            Kind::InvalidQuery { .. } => Some(
                "syntax: field=value, field>other_field, tags has x, field exists; combine with and, or, not and ( )".into(),
            ),
            Kind::Frontmatter { .. } => {
                Some("fix the YAML between the opening and closing --- lines".into())
            }
            Kind::Conversion { .. } => Some(
                "check `commonplace doctor` for converters, or convert by hand and use `source attach-text`"
                    .into(),
            ),
            Kind::InvalidInput { .. }
            | Kind::Io { .. }
            | Kind::IoUnlocated { .. }
            | Kind::Parse { .. }
            | Kind::ParseUnlocated { .. } => None,
        }
    }

    /// Close matches for a missing page, best first; empty for other errors.
    pub fn suggestions(&self) -> &[String] {
        match &self.kind {
            Kind::PageNotFound { suggestions, .. } => suggestions,
            _ => &[],
        }
    }

    /// The file the error is about, when known.
    pub fn path(&self) -> Option<&Path> {
        match &self.kind {
            Kind::Io { path, .. } | Kind::Parse { path, .. } => Some(path),
            Kind::Frontmatter { path, .. } => path.as_deref(),
            _ => None,
        }
    }

    /// True for `page_not_found`, `source_not_found`, `skill_not_found` and `text_layer_missing`.
    pub fn is_not_found(&self) -> bool {
        matches!(
            self.kind,
            Kind::PageNotFound { .. }
                | Kind::SourceNotFound { .. }
                | Kind::SkillNotFound { .. }
                | Kind::TextLayerMissing { .. }
        )
    }

    /// The backtrace captured when the error was created (empty unless
    /// `RUST_BACKTRACE` or `RUST_LIB_BACKTRACE` is set).
    pub fn backtrace(&self) -> &Backtrace {
        &self.backtrace
    }

    /// Every stable code, for documentation checks.
    pub const CODES: &'static [&'static str] = &[
        "page_not_found",
        "source_not_found",
        "skill_not_found",
        "text_layer_missing",
        "already_exists",
        "no_root",
        "invalid_query",
        "invalid_input",
        "frontmatter",
        "conversion",
        "io",
        "parse",
    ];
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.kind.fmt(f)
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code(), self.kind)?;
        if let Some(source) = std::error::Error::source(&self.kind) {
            write!(f, "\ncaused by: {source}")?;
        }
        if self.backtrace.status() == std::backtrace::BacktraceStatus::Captured {
            write!(f, "\n{}", self.backtrace)?;
        }
        Ok(())
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.kind {
            Kind::Io { source, .. } | Kind::IoUnlocated { source } => Some(source),
            Kind::Parse { source, .. } | Kind::ParseUnlocated { source } => Some(source.as_ref()),
            _ => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(source: std::io::Error) -> Self {
        Self::new(Kind::IoUnlocated { source })
    }
}

macro_rules! from_parse_error {
    ($($t:ty),*) => {$(
        impl From<$t> for Error {
            fn from(source: $t) -> Self {
                Self::new(Kind::ParseUnlocated { source: Box::new(source) })
            }
        }
    )*};
}
from_parse_error!(
    serde_yaml::Error,
    serde_json::Error,
    toml::de::Error,
    toml::ser::Error
);

#[cfg(test)]
mod tests {
    use super::*;

    fn one_of_each() -> Vec<Error> {
        let io = || std::io::Error::other("boom");
        vec![
            Error::page_not_found("x", vec![]),
            Error::source_not_found("x"),
            Error::skill_not_found("x"),
            Error::text_layer_missing("x"),
            Error::already_exists("x"),
            Error::no_root(Path::new("/x")),
            Error::invalid_query("x"),
            Error::invalid_input("x"),
            Error::frontmatter(None, "x"),
            Error::conversion("x"),
            Error::io("reading", Path::new("a.md"), io()),
            Error::from(io()),
            Error::parse(Path::new("a.yaml"), "bad"),
        ]
    }

    #[test]
    fn codes_list_matches_every_kind() {
        let mut seen: Vec<&str> = one_of_each().iter().map(Error::code).collect();
        seen.sort_unstable();
        seen.dedup();
        let mut listed = Error::CODES.to_vec();
        listed.sort_unstable();
        assert_eq!(seen, listed);
    }

    #[test]
    fn page_not_found_hints_best_suggestion() {
        let e = Error::page_not_found("Custmer", vec!["Customer".into(), "Custom".into()]);
        assert_eq!(e.to_string(), "no page named 'Custmer'");
        assert_eq!(e.hint().as_deref(), Some("did you mean 'Customer'?"));
        assert!(e.is_not_found());
    }

    #[test]
    fn io_error_names_operation_and_path() {
        let e = Error::io(
            "reading",
            Path::new("notes/a.md"),
            std::io::Error::other("denied"),
        );
        assert_eq!(e.to_string(), "reading notes/a.md: denied");
        assert_eq!(e.path(), Some(Path::new("notes/a.md")));
        assert!(std::error::Error::source(&e).is_some());
    }

    #[test]
    fn in_file_fills_missing_frontmatter_path_only() {
        let e = Error::frontmatter(None, "never closed").in_file(Path::new("p.md"));
        assert_eq!(e.to_string(), "p.md: never closed");
        let kept = Error::frontmatter(Some(Path::new("a.md")), "x").in_file(Path::new("b.md"));
        assert_eq!(kept.path(), Some(Path::new("a.md")));
    }

    #[test]
    fn codes_are_documented_for_agents() {
        let doc = include_str!("../../../skills/commonplace/references/cli.md");
        for code in Error::CODES {
            assert!(
                doc.contains(&format!("`{code}`")),
                "error code `{code}` missing from references/cli.md"
            );
        }
    }
}
