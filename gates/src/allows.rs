use proc_macro2::{Delimiter, TokenStream, TokenTree};

use crate::tokens::{calls, is_ident, is_punct, levels, squeezed, Source};
use crate::workspace::Workspace;
use crate::{Unread, Verdict};

const TEST_ALLOWS: [&str; 2] = ["clippy::expect_used", "clippy::panic"];

fn sanctioned(attribute: &TokenStream) -> bool {
    let trees: Vec<TokenTree> = attribute.clone().into_iter().collect();
    let [head, TokenTree::Group(listed)] = trees.as_slice() else {
        return false;
    };
    let inner: Vec<TokenTree> = listed.stream().into_iter().collect();
    let nested: bool = inner
        .iter()
        .any(|tree| matches!(tree, TokenTree::Group(group) if group.delimiter() == Delimiter::Parenthesis));
    let names: Vec<String> = inner
        .split(|tree| is_punct(tree, ','))
        .map(|name| name.iter().map(ToString::to_string).collect::<String>())
        .filter(|name| !name.is_empty())
        .collect();
    is_ident(head, "allow")
        && listed.delimiter() == Delimiter::Parenthesis
        && !nested
        && !names.is_empty()
        && names.iter().all(|name| TEST_ALLOWS.contains(&name.as_str()))
}

pub fn flagged(source: &Source, in_tests: bool) -> Result<Vec<String>, Unread> {
    let mut found: Vec<(usize, String)> = Vec::new();
    let mut inside: usize = 0;
    levels(&source.tokens, &mut |trees| {
        for (index, tree) in trees.iter().enumerate() {
            let inner: bool = trees.get(index + 1).is_some_and(|next| is_punct(next, '!'));
            let Some(TokenTree::Group(group)) = trees.get(index + 1 + usize::from(inner)) else {
                continue;
            };
            if !is_punct(tree, '#') || group.delimiter() != Delimiter::Bracket {
                continue;
            }
            let levels_named: Vec<String> = calls(&group.stream(), &["allow", "expect"]);
            inside += levels_named.iter().filter(|level| *level == "allow").count();
            if levels_named.is_empty() || (in_tests && inner && sanctioned(&group.stream())) {
                continue;
            }
            let written: &str = source.slice(group.span());
            let body: &str = written.get(1..written.len().saturating_sub(1)).unwrap_or("");
            let bang: &str = if inner { "!" } else { "" };
            found.push((
                tree.span().byte_range().start,
                format!("arch-tsu: {}:{} #{bang}[{}]", source.shown, tree.span().start().line, squeezed(body)),
            ));
        }
    });
    if inside != calls(&source.tokens, &["allow"]).len() {
        return Err(Unread(format!("{}: an `allow(` stands outside every attribute the scan read", source.shown)));
    }
    found.sort();
    Ok(found.into_iter().map(|flag| flag.1).collect())
}

pub fn allows(_asked: &[String]) -> Result<Verdict, Unread> {
    let workspace: Workspace = Workspace::read()?;
    let mut found: Vec<String> = Vec::new();
    for package in &workspace.packages {
        for path in workspace.rust_files(package, &[])? {
            let in_tests: bool = path.strip_prefix(package.folder()).is_ok_and(|inside| inside.starts_with("tests"));
            found.append(&mut flagged(&Source::read(&path, workspace.relative(&path))?, in_tests)?);
        }
    }
    let mut sorted: Vec<&str> = TEST_ALLOWS.to_vec();
    sorted.sort_unstable();
    Ok(Verdict {
        found,
        fix: vec![
            format!("The one allow is `#![allow({})]` opening a file under tests/.", sorted.join(", ")),
            String::from("Fix: remove the attribute and fix what the lint names (a type alias, a smaller function)."),
        ],
    })
}
