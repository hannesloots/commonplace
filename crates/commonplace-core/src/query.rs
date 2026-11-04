//! Dataview-like frontmatter queries.
//!
//! ```text
//! type=capability and target_maturity>baseline_maturity
//! status!=archived or tags has topic/law
//! folder~projects/matter-x and not confidence=low
//! ```
//!
//! Operators: `=` `!=` `>` `<` `>=` `<=` `~` (substring, case-insensitive),
//! `has` (list contains), `exists`. Right-hand side: quoted string, number, or
//! bare word. A bare word that names a field present on the page (or is
//! written `@field`) compares two fields. Virtual fields: `folder`, `path`,
//! `stem`, `title`, `type`, `wordcount`.

use crate::frontmatter;
use crate::vault::Page;
use crate::{Error, Result};
use serde_yaml::Value;

#[derive(Debug, Clone)]
enum Tok {
    Ident(String),
    Str(String),
    Op(String),
    And,
    Or,
    Not,
    LParen,
    RParen,
}

fn lex(input: &str) -> Result<Vec<Tok>> {
    let mut out = vec![];
    let cs: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        // backticks are markdown code marks (older help text showed them): skip
        if c.is_whitespace() || c == '`' {
            i += 1;
            continue;
        }
        if c == '(' {
            out.push(Tok::LParen);
            i += 1;
            continue;
        }
        if c == ')' {
            out.push(Tok::RParen);
            i += 1;
            continue;
        }
        if c == '"' || c == '\'' {
            let quote = c;
            let mut j = i + 1;
            let mut value = String::new();
            while j < cs.len() && cs[j] != quote {
                value.push(cs[j]);
                j += 1;
            }
            if j >= cs.len() {
                return Err(Error::invalid_query("unterminated string in query"));
            }
            out.push(Tok::Str(value));
            i = j + 1;
            continue;
        }
        if "=!<>~".contains(c) {
            let mut op = String::from(c);
            if i + 1 < cs.len() && cs[i + 1] == '=' {
                op.push('=');
                i += 1;
            }
            i += 1;
            if op == "==" {
                op = "=".into();
            }
            out.push(Tok::Op(op));
            continue;
        }
        let mut j = i;
        let mut word = String::new();
        while j < cs.len() && !cs[j].is_whitespace() && !"()=!<>~\"'`".contains(cs[j]) {
            word.push(cs[j]);
            j += 1;
        }
        i = j;
        match word.to_ascii_lowercase().as_str() {
            "and" => out.push(Tok::And),
            "or" => out.push(Tok::Or),
            "not" => out.push(Tok::Not),
            "has" | "exists" => out.push(Tok::Op(word.to_ascii_lowercase())),
            _ => out.push(Tok::Ident(word)),
        }
    }
    Ok(out)
}

/// A parsed `query --where` expression.
#[derive(Debug, Clone)]
pub enum Expr {
    /// `field op rhs`, e.g. `status = seed`.
    Cmp {
        /// The field compared (see [`field_value`]).
        field: String,
        /// Operator: `=`, `!=`, `<`, `<=`, `>`, `>=`, `~` (contains), `has` or `exists`.
        op: String,
        /// What the field is compared with.
        rhs: Rhs,
    },
    /// Both sides hold.
    And(Box<Expr>, Box<Expr>),
    /// Either side holds.
    Or(Box<Expr>, Box<Expr>),
    /// The inner expression does not hold.
    Not(Box<Expr>),
}

/// The right-hand side of a comparison.
#[derive(Debug, Clone)]
pub enum Rhs {
    /// A quoted literal.
    Lit(String),
    /// Another field's value, written `@field`.
    Field(String),
    /// An unquoted word, compared as text.
    Bare(String),
    /// No value: the `exists` operator.
    None,
}

struct Parser {
    toks: Vec<Tok>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }
    fn next(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.pos).cloned();
        self.pos += 1;
        t
    }
    fn expr(&mut self) -> Result<Expr> {
        let mut left = self.term()?;
        while let Some(Tok::Or) = self.peek() {
            self.next();
            let r = self.term()?;
            left = Expr::Or(Box::new(left), Box::new(r));
        }
        Ok(left)
    }
    fn term(&mut self) -> Result<Expr> {
        let mut left = self.factor()?;
        while let Some(Tok::And) = self.peek() {
            self.next();
            let r = self.factor()?;
            left = Expr::And(Box::new(left), Box::new(r));
        }
        Ok(left)
    }
    fn factor(&mut self) -> Result<Expr> {
        match self.next() {
            Some(Tok::Not) => Ok(Expr::Not(Box::new(self.factor()?))),
            Some(Tok::LParen) => {
                let e = self.expr()?;
                match self.next() {
                    Some(Tok::RParen) => Ok(e),
                    _ => Err(Error::invalid_query("expected )")),
                }
            }
            Some(Tok::Ident(field)) => {
                let Some(Tok::Op(op)) = self.next() else {
                    return Err(Error::invalid_query(format!(
                        "expected operator after '{field}'"
                    )));
                };
                if op == "exists" {
                    return Ok(Expr::Cmp {
                        field,
                        op,
                        rhs: Rhs::None,
                    });
                }
                let rhs = match self.next() {
                    Some(Tok::Str(s)) => Rhs::Lit(s),
                    Some(Tok::Ident(w)) => {
                        if let Some(f) = w.strip_prefix('@') {
                            Rhs::Field(f.to_string())
                        } else {
                            Rhs::Bare(w)
                        }
                    }
                    _ => {
                        return Err(Error::invalid_query(format!(
                            "expected value after '{field} {op}'"
                        )));
                    }
                };
                Ok(Expr::Cmp { field, op, rhs })
            }
            other => Err(Error::invalid_query(format!("unexpected token {other:?}"))),
        }
    }
}

/// Parse a `--where` expression.
///
/// # Errors
///
/// `invalid_query` when the expression does not parse.
pub fn parse(s: &str) -> Result<Expr> {
    let toks = lex(s)?;
    if toks.is_empty() {
        return Err(Error::invalid_query("empty query"));
    }
    let mut p = Parser { toks, pos: 0 };
    let e = p.expr()?;
    if p.pos < p.toks.len() {
        return Err(Error::invalid_query("unexpected trailing tokens in query"));
    }
    Ok(e)
}

/// The value of `field` on `page`, including computed fields such as `type` and `path`.
pub fn field_value(page: &Page, field: &str) -> Option<Value> {
    match field {
        "folder" => Some(Value::String(page.folder())),
        "path" => Some(Value::String(page.rel.clone())),
        "stem" | "name" => Some(Value::String(page.stem.clone())),
        "title" => Some(Value::String(page.title.clone())),
        "type" => Some(Value::String(page.ptype.clone())),
        "wordcount" => Some(Value::Number(
            crate::markdown::word_count(&page.doc.body).into(),
        )),
        _ => page.fm().get(field).cloned(),
    }
}

fn scalar(v: &Value) -> String {
    frontmatter::scalar_to_string(v)
}

fn cmp_values(a: &str, op: &str, b: &str) -> bool {
    use std::cmp::Ordering::{Equal, Greater, Less};
    let (na, nb) = (a.trim().parse::<f64>(), b.trim().parse::<f64>());
    let ord = match (na, nb) {
        (Ok(x), Ok(y)) => x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal),
        _ => a.to_lowercase().cmp(&b.to_lowercase()),
    };
    match op {
        "=" => ord == Equal,
        "!=" => ord != Equal,
        ">" => ord == Greater,
        "<" => ord == Less,
        ">=" => ord != Less,
        "<=" => ord != Greater,
        "~" => a.to_lowercase().contains(&b.to_lowercase()),
        _ => false,
    }
}

/// Whether `page` satisfies `e`.
pub fn eval(e: &Expr, page: &Page) -> bool {
    match e {
        Expr::And(a, b) => eval(a, page) && eval(b, page),
        Expr::Or(a, b) => eval(a, page) || eval(b, page),
        Expr::Not(a) => !eval(a, page),
        Expr::Cmp { field, op, rhs } => {
            let lv = field_value(page, field);
            if op == "exists" {
                return lv.is_some_and(|v| !v.is_null());
            }
            let rv: Option<String> = match rhs {
                Rhs::Lit(s) => Some(s.clone()),
                Rhs::Field(f) => field_value(page, f).map(|v| scalar(&v)),
                Rhs::Bare(w) => match field_value(page, w) {
                    Some(v) if !matches!(v, Value::Null) => Some(scalar(&v)),
                    _ => Some(w.clone()),
                },
                Rhs::None => None,
            };
            let Some(rv) = rv else { return false };
            match (lv, op.as_str()) {
                (None, "!=") => true,
                (None, _) => false,
                (Some(Value::Sequence(seq)), "has") => seq.iter().any(|x| {
                    scalar(x).eq_ignore_ascii_case(&rv)
                        || scalar(x).to_lowercase().contains(&rv.to_lowercase())
                }),
                (Some(v), "has") => scalar(&v).to_lowercase().contains(&rv.to_lowercase()),
                (Some(Value::Sequence(seq)), "=") => {
                    seq.iter().any(|x| scalar(x).eq_ignore_ascii_case(&rv))
                }
                (Some(Value::Sequence(seq)), "!=") => {
                    !seq.iter().any(|x| scalar(x).eq_ignore_ascii_case(&rv))
                }
                (Some(v), op) => cmp_values(&scalar(&v), op, &rv),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses() {
        parse("type=capability and target_maturity>baseline_maturity").unwrap();
        parse("status!=archived or tags has topic/law").unwrap();
        parse("not (confidence=low) and folder~projects").unwrap();
        parse("verified exists").unwrap();
        assert!(parse("type=").is_err());
    }
}
