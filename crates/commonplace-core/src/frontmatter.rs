//! YAML frontmatter split/parse/serialise. The mapping keeps insertion order so
//! rewriting a page does not reshuffle its properties.

use anyhow::{anyhow, Result};
use serde_yaml::{Mapping, Value};

/// A parsed page: frontmatter mapping (may be empty) and body text.
#[derive(Debug, Clone, Default)]
pub struct Document {
    pub fm: Mapping,
    pub has_frontmatter: bool,
    pub body: String,
    /// Number of lines occupied by the frontmatter block (including fences), so
    /// body line numbers can be reported as file line numbers.
    pub body_line_offset: usize,
}

pub fn parse(text: &str) -> Result<Document> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    if !(text.starts_with("---\n") || text.starts_with("---\r\n")) {
        return Ok(Document { fm: Mapping::new(), has_frontmatter: false, body: text.to_string(), body_line_offset: 0 });
    }
    // find closing fence on its own line
    let mut offset = 0usize;
    let mut lines = text.split_inclusive('\n');
    let first = lines.next().unwrap();
    offset += first.len();
    let mut yaml = String::new();
    let mut closed = false;
    let mut fm_lines = 1usize;
    for line in lines {
        fm_lines += 1;
        offset += line.len();
        let t = line.trim_end_matches(['\n', '\r']);
        if t == "---" || t == "..." {
            closed = true;
            break;
        }
        yaml.push_str(line);
    }
    if !closed {
        return Err(anyhow!("frontmatter opened with --- but never closed"));
    }
    let fm: Mapping = if yaml.trim().is_empty() {
        Mapping::new()
    } else {
        match serde_yaml::from_str::<Value>(&yaml)? {
            Value::Mapping(m) => m,
            Value::Null => Mapping::new(),
            other => return Err(anyhow!("frontmatter is not a mapping: {:?}", other)),
        }
    };
    Ok(Document { fm, has_frontmatter: true, body: text[offset..].to_string(), body_line_offset: fm_lines })
}

pub fn render(fm: &Mapping, body: &str) -> Result<String> {
    let mut out = String::from("---\n");
    if !fm.is_empty() {
        let y = serde_yaml::to_string(&Value::Mapping(fm.clone()))?;
        out.push_str(&y);
        if !y.ends_with('\n') {
            out.push('\n');
        }
    }
    out.push_str("---\n");
    out.push_str(body);
    Ok(out)
}

pub fn get_str(fm: &Mapping, key: &str) -> Option<String> {
    match fm.get(key)? {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Tagged(t) => match &t.value {
            Value::String(s) => Some(s.clone()),
            v => Some(scalar_to_string(v)),
        },
        Value::Null => None,
        v => Some(scalar_to_string(v)),
    }
}

pub fn scalar_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => String::new(),
        Value::Sequence(seq) => seq.iter().map(scalar_to_string).collect::<Vec<_>>().join(", "),
        Value::Mapping(_) => "{...}".to_string(),
        Value::Tagged(t) => scalar_to_string(&t.value),
    }
}

/// Return a list-valued field as strings. A scalar becomes a one-element list.
pub fn get_list(fm: &Mapping, key: &str) -> Vec<String> {
    match fm.get(key) {
        None | Some(Value::Null) => vec![],
        Some(Value::Sequence(seq)) => seq.iter().map(scalar_to_string).filter(|s| !s.is_empty()).collect(),
        Some(v) => {
            let s = scalar_to_string(v);
            if s.is_empty() { vec![] } else { vec![s] }
        }
    }
}

pub fn get_bool(fm: &Mapping, key: &str) -> Option<bool> {
    match fm.get(key)? {
        Value::Bool(b) => Some(*b),
        Value::String(s) => match s.to_ascii_lowercase().as_str() {
            "true" | "yes" => Some(true),
            "false" | "no" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

pub fn set_str(fm: &mut Mapping, key: &str, val: &str) {
    fm.insert(Value::String(key.to_string()), Value::String(val.to_string()));
}

pub fn set_value(fm: &mut Mapping, key: &str, val: Value) {
    fm.insert(Value::String(key.to_string()), val);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let t = "---\ntype: concept\ntags: [a, b]\n---\n# Hi\nbody\n";
        let d = parse(t).unwrap();
        assert_eq!(get_str(&d.fm, "type").as_deref(), Some("concept"));
        assert_eq!(get_list(&d.fm, "tags"), vec!["a", "b"]);
        assert_eq!(d.body, "# Hi\nbody\n");
        assert_eq!(d.body_line_offset, 4);
        let r = render(&d.fm, &d.body).unwrap();
        assert!(r.starts_with("---\ntype: concept\n"));
    }

    #[test]
    fn no_frontmatter() {
        let d = parse("# Title\n").unwrap();
        assert!(!d.has_frontmatter);
        assert_eq!(d.body_line_offset, 0);
    }
}
