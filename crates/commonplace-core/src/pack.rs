//! Domain packs: declarative types, lint rules and traces loaded from
//! `.commonplace/packs/<name>/pack.yaml`. The binary has no domain knowledge; packs
//! are data, so a user can add or edit one without touching Rust.

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Pack {
    pub name: String,
    pub version: u32,
    pub description: String,
    pub types: BTreeMap<String, TypeSpec>,
    pub rules: Vec<Rule>,
    pub traces: Vec<Trace>,
    pub personas: Vec<String>,
    pub workflows: Vec<String>,
    pub folders: Vec<String>,
    /// The jurisdiction profile that filled this pack's placeholders, if the
    /// pack has a `jurisdictions/` folder (legal pack).
    #[serde(skip)]
    pub jurisdiction: Option<serde_yaml::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct TypeSpec {
    pub description: String,
    pub folder_hint: String,
    pub required: Vec<String>,
    pub enums: BTreeMap<String, Vec<String>>,
    /// Allowed `status` values for this type (overrides the core default).
    pub status: Vec<String>,
    pub links: BTreeMap<String, LinkSpec>,
    /// Whether pages of this type must carry sources/citations like synthesis pages.
    pub cited: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct LinkSpec {
    pub to: Vec<String>,
    pub min: usize,
    pub severity: Option<String>,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Rule {
    pub id: String,
    pub severity: String,
    pub when: When,
    pub check: String,
    pub message: String,
    // parameters (used by different checks)
    pub field: String,
    pub fields: Vec<String>,
    pub values: Vec<String>,
    pub to: Vec<String>,
    pub min: usize,
    pub op: String,
    pub other: String,
    pub pattern: String,
    pub page_type: String,
    pub page_field: String,
    pub page_must: BTreeMap<String, String>,
    pub years_back: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct When {
    #[serde(rename = "type")]
    pub ptype: Option<String>,
    pub folder: Option<String>,
    pub status_not: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Trace {
    pub name: String,
    pub description: String,
    /// Alternating: type, link field, type, link field, type ...
    pub chain: Vec<String>,
}

/// Load enabled packs. A pack with a `jurisdictions/` folder (the legal pack)
/// is a template: `{{jurisdiction.key}}` placeholders in its pack.yaml are
/// filled from `jurisdictions/<code>.yaml`, where `code` is `jurisdiction` in
/// commonplace.toml (`generic` when unset). A list item that is exactly a
/// placeholder naming a list in the profile is spliced (court codes).
pub fn load_packs(packs_dir: &Path, names: &[String], jurisdiction: &str) -> Result<Vec<Pack>> {
    let mut out = vec![];
    for raw_name in names {
        let (mut n, _) = crate::scaffold::normalise_pack(raw_name);
        let mut p = packs_dir.join(&n).join("pack.yaml");
        if !p.is_file() && n != raw_name.trim() {
            // a commonplace set up before the rename still has the pack under its old name
            let legacy = packs_dir.join(raw_name.trim()).join("pack.yaml");
            if legacy.is_file() {
                n = raw_name.trim().to_string();
                p = legacy;
            }
        }
        if !p.is_file() {
            continue;
        }
        let s = std::fs::read_to_string(&p).with_context(|| format!("reading {}", p.display()))?;
        let mut value: serde_yaml::Value =
            serde_yaml::from_str(&s).with_context(|| format!("parsing {}", p.display()))?;
        let mut profile = None;
        let jdir = packs_dir.join(&n).join("jurisdictions");
        if jdir.is_dir() {
            let code = if jurisdiction.is_empty() {
                "generic"
            } else {
                jurisdiction
            };
            let jp = jdir.join(format!("{code}.yaml"));
            if !jp.is_file() {
                let available = available_jurisdictions(&jdir).join(", ");
                bail!(
                    "pack '{n}' has no jurisdiction profile '{code}' (expected {}); available: {available}. Set `jurisdiction` in commonplace.toml or add the profile",
                    jp.display()
                );
            }
            let js = std::fs::read_to_string(&jp)
                .with_context(|| format!("reading {}", jp.display()))?;
            let jv: serde_yaml::Value =
                serde_yaml::from_str(&js).with_context(|| format!("parsing {}", jp.display()))?;
            substitute(&mut value, &jv);
            profile = Some(jv);
        }
        let mut pack: Pack =
            serde_yaml::from_value(value).with_context(|| format!("parsing {}", p.display()))?;
        if pack.name.is_empty() {
            pack.name = n.clone();
        }
        pack.jurisdiction = profile;
        out.push(pack);
    }
    Ok(out)
}

/// Profile codes available in a pack's `jurisdictions/` folder.
pub fn available_jurisdictions(jdir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(jdir)
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| {
                    e.path()
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                })
                .filter(|s| s != "generic")
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

static PLACEHOLDER_RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    regex::Regex::new(r"\{\{jurisdiction\.([A-Za-z0-9_.]+)\}\}").unwrap()
});

fn profile_get<'a>(profile: &'a serde_yaml::Value, dotted: &str) -> Option<&'a serde_yaml::Value> {
    let mut cur = profile;
    for key in dotted.split('.') {
        cur = cur.get(key)?;
    }
    Some(cur)
}

fn scalar_text(v: &serde_yaml::Value) -> String {
    match v {
        serde_yaml::Value::String(s) => s.clone(),
        serde_yaml::Value::Number(n) => n.to_string(),
        serde_yaml::Value::Bool(b) => b.to_string(),
        serde_yaml::Value::Sequence(seq) => {
            seq.iter().map(scalar_text).collect::<Vec<_>>().join(", ")
        }
        _ => String::new(),
    }
}

fn substitute(v: &mut serde_yaml::Value, profile: &serde_yaml::Value) {
    match v {
        serde_yaml::Value::String(s) => {
            if s.contains("{{jurisdiction.") {
                *s = PLACEHOLDER_RE
                    .replace_all(s, |c: &regex::Captures| {
                        profile_get(profile, &c[1]).map_or_else(|| c[0].to_string(), scalar_text)
                    })
                    .into_owned();
            }
        }
        serde_yaml::Value::Sequence(seq) => {
            let mut out = Vec::with_capacity(seq.len());
            for mut item in seq.drain(..) {
                if let serde_yaml::Value::String(s) = &item
                    && let Some(c) = PLACEHOLDER_RE.captures(s)
                    && c[0].len() == s.len()
                    && let Some(serde_yaml::Value::Sequence(items)) = profile_get(profile, &c[1])
                {
                    out.extend(items.iter().cloned());
                    continue;
                }
                substitute(&mut item, profile);
                out.push(item);
            }
            *seq = out;
        }
        serde_yaml::Value::Mapping(m) => {
            for (_, val) in m.iter_mut() {
                substitute(val, profile);
            }
        }
        _ => {}
    }
}

/// Simple glob: `*` matches within a path segment, `**` matches across segments.
pub fn glob_match(pattern: &str, path: &str) -> bool {
    fn to_regex(p: &str) -> String {
        let mut r = String::from("^");
        let mut chars = p.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '*' => {
                    if chars.peek() == Some(&'*') {
                        chars.next();
                        if chars.peek() == Some(&'/') {
                            chars.next();
                            r.push_str("(?:.*/)?");
                        } else {
                            r.push_str(".*");
                        }
                    } else {
                        r.push_str("[^/]*");
                    }
                }
                '?' => r.push_str("[^/]"),
                '.' | '(' | ')' | '+' | '|' | '^' | '$' | '{' | '}' | '[' | ']' | '\\' => {
                    r.push('\\');
                    r.push(c);
                }
                c => r.push(c),
            }
        }
        r.push('$');
        r
    }
    regex::Regex::new(&to_regex(pattern)).is_ok_and(|re| re.is_match(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substitutes_scalars_and_splices_lists() {
        let profile: serde_yaml::Value =
            serde_yaml::from_str("courts: [A, B]\nsrc: SAFLII\nex:\n  neutral: '[2025] X 1'\n")
                .unwrap();
        let mut v: serde_yaml::Value = serde_yaml::from_str("enums:\n  court: ['{{jurisdiction.courts}}', other]\nmsg: 'verify on {{jurisdiction.src}} ({{jurisdiction.ex.neutral}})'\nkeep: '{{jurisdiction.missing}}'\n").unwrap();
        substitute(&mut v, &profile);
        let courts: Vec<String> = v["enums"]["court"]
            .as_sequence()
            .unwrap()
            .iter()
            .map(|x| x.as_str().unwrap().to_string())
            .collect();
        assert_eq!(courts, vec!["A", "B", "other"]);
        assert_eq!(v["msg"].as_str().unwrap(), "verify on SAFLII ([2025] X 1)");
        assert_eq!(v["keep"].as_str().unwrap(), "{{jurisdiction.missing}}");
    }

    #[test]
    fn globs() {
        assert!(glob_match("projects/*/drafts/**", "projects/x/drafts/a.md"));
        assert!(glob_match(
            "projects/*/drafts/**",
            "projects/x/drafts/sub/a.md"
        ));
        assert!(!glob_match("projects/*/drafts/**", "projects/x/notes/a.md"));
        assert!(glob_match("**/*.md", "a/b/c.md"));
        assert!(glob_match("archive/**", "archive/x.md"));
    }
}
