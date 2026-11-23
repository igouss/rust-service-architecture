use proc_macro2::{Delimiter, TokenStream, TokenTree};

use crate::tokens::{is_ident, is_punct, levels, paths, Source};
use crate::workspace::Workspace;
use crate::{Unread, Verdict};

/// An `impl TryFrom<…> for Holder` and the tokens of its body.
pub struct Loader {
    pub holder: String,
    pub body: TokenStream,
}

fn marks(tree: &TokenTree) -> bool {
    let TokenTree::Group(attribute) = tree else {
        return false;
    };
    let inner: Vec<TokenTree> = attribute.stream().into_iter().collect();
    let [first, TokenTree::Group(given)] = inner.as_slice() else {
        return false;
    };
    let given: Vec<TokenTree> = given.stream().into_iter().collect();
    attribute.delimiter() == Delimiter::Bracket
        && is_ident(first, "serde")
        && given.windows(2).any(|pair| is_ident(&pair[0], "try_from") && is_punct(&pair[1], '='))
}

fn declared(after: &[TokenTree]) -> Option<String> {
    let at: usize = after.iter().position(|tree| {
        is_ident(tree, "struct")
            || is_ident(tree, "enum")
            || is_punct(tree, ';')
            || matches!(tree, TokenTree::Group(group) if group.delimiter() == Delimiter::Brace)
    })?;
    match (after.get(at), after.get(at + 1)) {
        (Some(keyword), Some(TokenTree::Ident(name))) if is_ident(keyword, "struct") || is_ident(keyword, "enum") => Some(name.to_string()),
        _ => None,
    }
}

/// The structs and enums the file marks `#[serde(try_from = …)]`: the types whose stored values load through `TryFrom`.
pub fn marked(source: &Source) -> Result<Vec<String>, Unread> {
    let mut written: usize = 0;
    let mut names: Vec<String> = Vec::new();
    levels(&source.tokens, &mut |trees| {
        written += trees
            .windows(2)
            .filter(|pair| is_ident(&pair[0], "try_from") && is_punct(&pair[1], '='))
            .count();
        for (at, tree) in trees.iter().enumerate() {
            if marks(tree) {
                names.extend(declared(&trees[at + 1..]));
            }
        }
    });
    if written != names.len() {
        return Err(Unread(format!(
            "{}: {written} `try_from =` written, {} read as `#[serde(try_from = …)]` on a struct or an enum",
            source.shown,
            names.len()
        )));
    }
    Ok(names)
}

fn loader(trees: &[TokenTree]) -> Option<Loader> {
    let to: usize = trees.iter().position(|tree| is_ident(tree, "for") || matches!(tree, TokenTree::Group(_)))?;
    match (trees.get(to), trees.get(to + 1), trees.get(to + 2)) {
        (Some(keyword), Some(TokenTree::Ident(holder)), Some(TokenTree::Group(body))) if is_ident(keyword, "for") && body.delimiter() == Delimiter::Brace => {
            Some(Loader {
                holder: holder.to_string(),
                body: body.stream(),
            })
        }
        _ => None,
    }
}

fn heads(after: &[TokenTree]) -> bool {
    after
        .iter()
        .find(|tree| is_ident(tree, "for") || is_punct(tree, ';') || matches!(tree, TokenTree::Group(_)))
        .is_some_and(|tree| is_ident(tree, "for"))
}

/// Every `impl TryFrom<…> for Holder { … }` in the file; unread when one is written in another shape.
pub fn loaders(source: &Source) -> Result<Vec<Loader>, Unread> {
    let mut written: usize = 0;
    let mut found: Vec<Loader> = Vec::new();
    levels(&source.tokens, &mut |trees| {
        for (at, tree) in trees.iter().enumerate() {
            if is_ident(tree, "TryFrom") && heads(&trees[at + 1..]) {
                written += 1;
                if at > 0 && is_ident(&trees[at - 1], "impl") {
                    found.extend(loader(&trees[at + 1..]));
                }
            }
        }
    });
    if written != found.len() {
        return Err(Unread(format!(
            "{}: {written} `TryFrom<…> for` written, {} read as `impl TryFrom<…> for Type {{ … }}`",
            source.shown,
            found.len()
        )));
    }
    Ok(found)
}

/// One line for each marked type whose loader calls the type's `new`; unread when a marked type has no loader.
pub fn flagged(member: &str, marked: &[String], loaders: &[Loader]) -> Result<Vec<String>, Unread> {
    let mut lines: Vec<String> = Vec::new();
    for name in marked {
        let own: Vec<&Loader> = loaders.iter().filter(|loader| &loader.holder == name).collect();
        if own.is_empty() {
            return Err(Unread(format!(
                "{member}: {name} is marked `try_from` and no `impl TryFrom<…> for {name}` is read in the member"
            )));
        }
        for loader in own {
            let called: bool = [name.as_str(), "Self"]
                .iter()
                .any(|head| paths(&loader.body, head).iter().any(|path| path == "new"));
            if called {
                lines.push(format!(
                    "arch-v9j1: {member}: {name} loads its stored values through TryFrom, which calls {name}::new"
                ));
            }
        }
    }
    Ok(lines)
}

pub fn refined(_asked: &[String]) -> Result<Verdict, Unread> {
    let workspace: Workspace = Workspace::read()?;
    let mut found: Vec<String> = Vec::new();
    for package in &workspace.packages {
        let mut names: Vec<String> = Vec::new();
        let mut held: Vec<Loader> = Vec::new();
        for path in workspace.rust_files(package, &["src"])? {
            let source: Source = Source::read(&path, workspace.relative(&path))?;
            names.append(&mut marked(&source)?);
            held.append(&mut loaders(&source)?);
        }
        found.append(&mut flagged(&package.name, &names, &held)?);
    }
    Ok(Verdict {
        found,
        fix: vec![
            String::from("`new` holds the rule for new input; TryFrom holds the rule stored values load under, and the two change apart."),
            String::from("Fix: write the stored rule out in `try_from`. A stored value must load whatever `new` accepts today."),
        ],
    })
}
