use std::ops::Range;

use proc_macro2::{LexError, TokenStream, TokenTree};

use crate::workspace::Workspace;
use crate::{Unread, Verdict};

pub const CORE: [&str; 2] = ["domain", "kernel"];

#[derive(Debug, PartialEq, Eq)]
pub struct Comment {
    pub line: usize,
    pub text: String,
}

fn covered(stream: &TokenStream, text: &str, into: &mut Vec<Range<usize>>) {
    for tree in stream.clone() {
        let whole: Range<usize> = tree.span().byte_range();
        let written: &str = text.get(whole.clone()).unwrap_or("");
        if written.starts_with("//") || written.starts_with("/*") {
            continue;
        }
        match tree {
            TokenTree::Group(group) => {
                into.push(group.span_open().byte_range());
                covered(&group.stream(), text, into);
                into.push(group.span_close().byte_range());
            }
            TokenTree::Ident(_) | TokenTree::Punct(_) | TokenTree::Literal(_) => into.push(whole),
        }
    }
}

fn block(rest: &str) -> Option<usize> {
    let mut depth: usize = 0;
    let mut at: usize = 0;
    while at < rest.len() {
        let ahead: &str = rest.get(at..).unwrap_or("");
        if ahead.starts_with("/*") {
            depth += 1;
            at += 2;
        } else if ahead.starts_with("*/") {
            depth -= 1;
            at += 2;
            if depth == 0 {
                return Some(at);
            }
        } else {
            at += ahead.chars().next().map_or(1, char::len_utf8);
        }
    }
    None
}

fn between(shown: &str, text: &str, gap: Range<usize>, found: &mut Vec<(usize, String)>) -> Result<(), Unread> {
    let mut at: usize = gap.start;
    while at < gap.end {
        let rest: &str = text.get(at..gap.end).unwrap_or("");
        let ends: Option<usize> = if rest.starts_with("//") {
            Some(rest.find('\n').unwrap_or(rest.len()))
        } else if rest.starts_with("/*") {
            block(rest)
        } else {
            None
        };
        match (ends, rest.chars().next()) {
            (Some(length), _) => {
                found.push((at, rest.get(..length).unwrap_or("").to_string()));
                at += length;
            }
            (None, Some(blank)) if blank.is_whitespace() => at += blank.len_utf8(),
            (None, _) => {
                let line: usize = 1 + text.get(..at).unwrap_or("").matches('\n').count();
                return Err(Unread(format!(
                    "{shown}:{line}: text between tokens that is neither blank nor a comment; the scan cannot read this file"
                )));
            }
        }
    }
    Ok(())
}

fn unlexed(shown: &str, text: &str, error: &LexError) -> Unread {
    let at: usize = error.span().byte_range().start;
    if text.get(at..).unwrap_or("").starts_with("/*") {
        let line: usize = 1 + text.get(..at).unwrap_or("").matches('\n').count();
        return Unread(format!("{shown}:{line}: a block comment does not end; the scan cannot read this file"));
    }
    Unread(format!("{shown}: {error}; the scan cannot read this file"))
}

/// Every comment in the text, doc comments included: whatever lies between two tokens and is not blank.
pub fn comments_in(shown: &str, text: &str) -> Result<Vec<Comment>, Unread> {
    let tokens: TokenStream = text.parse().map_err(|error: LexError| unlexed(shown, text, &error))?;
    let mut held: Vec<Range<usize>> = Vec::new();
    covered(&tokens, text, &mut held);
    held.sort_by_key(|range| (range.start, range.end));
    let mut found: Vec<(usize, String)> = Vec::new();
    let mut at: usize = 0;
    for range in held {
        if range.start > at {
            between(shown, text, at..range.start, &mut found)?;
        }
        at = at.max(range.end);
    }
    between(shown, text, at..text.len(), &mut found)?;
    Ok(found
        .into_iter()
        .map(|(offset, comment)| Comment {
            line: 1 + text.get(..offset).unwrap_or("").matches('\n').count(),
            text: comment,
        })
        .collect())
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
