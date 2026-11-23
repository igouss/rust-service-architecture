use ra_ap_rustc_lexer::{tokenize, FrontmatterAllowed, TokenKind};

use crate::tokens::Source;
use crate::workspace::Workspace;
use crate::{Unread, Verdict};

pub const CORE: [&str; 2] = ["domain", "kernel"];

#[derive(Debug, PartialEq, Eq)]
pub struct Comment {
    pub line: usize,
    pub text: String,
}

pub fn comments_in(shown: &str, text: &str) -> Result<Vec<Comment>, Unread> {
    let mut found: Vec<Comment> = Vec::new();
    let mut at: usize = 0;
    let mut line: usize = 1;
    for token in tokenize(text, FrontmatterAllowed::No) {
        let end: usize = at + token.len as usize;
        let piece: &str = text.get(at..end).unwrap_or("");
        if matches!(token.kind, TokenKind::BlockComment { terminated: false, .. }) {
            return Err(Unread(format!("{shown}:{line}: a block comment does not end; the scan cannot read this file")));
        }
        if matches!(token.kind, TokenKind::LineComment { .. } | TokenKind::BlockComment { .. }) {
            found.push(Comment { line, text: piece.to_string() });
        }
        line += piece.matches('\n').count();
        at = end;
    }
    Source::of(shown.to_string(), text.to_string())?;
    Ok(found)
}

fn kept(text: &str, core: bool) -> bool {
    let doc: bool = (text.starts_with("///") && !text.starts_with("////")) || text.starts_with("//!");
    let vouched: bool = core
        && text
            .strip_prefix("// fc-allow: ")
            .and_then(|reason| reason.chars().next())
            .is_some_and(|first| !first.is_whitespace());
    doc || vouched
}

pub fn flagged(shown: &str, text: &str, core: bool) -> Result<Vec<String>, Unread> {
    Ok(comments_in(shown, text)?
        .iter()
        .filter(|comment| !kept(&comment.text, core))
        .map(|comment| format!("arch-0d2: {shown}:{} {}", comment.line, comment.text.lines().next().unwrap_or("")))
        .collect())
}

pub fn comments(_asked: &[String]) -> Result<Verdict, Unread> {
    let workspace: Workspace = Workspace::read()?;
    let mut found: Vec<String> = Vec::new();
    for package in &workspace.packages {
        for path in workspace.rust_files(package, &[])? {
            let shown: String = workspace.relative(&path);
            let text: String = std::fs::read_to_string(&path).map_err(|error| Unread(format!("{shown}: {error}")))?;
            found.append(&mut flagged(&shown, &text, package.has_role(&CORE))?);
        }
    }
    Ok(Verdict {
        found,
        fix: vec![
            String::from("Names and types carry the meaning; `// fc-allow: <reason>` is read in the core only."),
            String::from("Fix: remove the comment, or make it a contract in a `///` doc comment on the item it binds."),
        ],
    })
}
