use std::collections::BTreeSet;
use std::path::PathBuf;

use proc_macro2::{Delimiter, TokenStream, TokenTree};

use crate::declared::{impls, slot, Head, Member};
use crate::tokens::{followed, is_ident, is_punct, levels, Source};
use crate::workspace::Workspace;
use crate::{Unread, Verdict};

const STORED: [(&str, &[&str]); 3] = [("Lifecycle", &["Event", "Refusal"]), ("Reaction", &["Request"]), ("Projection", &["Row"])];

fn kinds() -> Vec<&'static str> {
    STORED.iter().map(|stored| stored.0).collect()
}

fn direct(member: &Member) -> Result<BTreeSet<String>, Unread> {
    let mut named: BTreeSet<String> = BTreeSet::new();
    for (source, file) in &member.files {
        let read: Vec<crate::declared::Impl> = impls(file, &kinds());
        for held in &read {
            let slots: &[&str] = STORED.iter().find(|stored| stored.0 == held.kind).map_or(&[], |stored| stored.1);
            let given: Vec<Option<String>> = slots.iter().map(|name| slot(held.item, name)).collect();
            if given.contains(&None) {
                return Err(Unread(format!(
                    "{}: `impl {} for {}` does not name its {} as one type",
                    member.name,
                    held.kind,
                    held.holder,
                    slots.join(" and ")
                )));
            }
            named.extend(given.into_iter().flatten());
            if held.kind == "Lifecycle" {
                named.insert(held.holder.clone());
            }
        }
        let written: usize = kinds().iter().map(|kind| followed(&source.tokens, kind, "for")).sum();
        if written != read.len() {
            return Err(Unread(format!("{}: an impl of {} the scan could not read", member.name, kinds().join(", "))));
        }
    }
    Ok(named)
}

fn reached(member: &Member, direct: &BTreeSet<String>) -> Result<BTreeSet<String>, Unread> {
    let mut reached: BTreeSet<String> = BTreeSet::new();
    let mut queue: Vec<String> = direct.iter().cloned().collect();
    while let Some(name) = queue.pop() {
        if reached.insert(name.clone()) {
            queue.extend(
                member
                    .declared(&name)?
                    .mentions()
                    .into_iter()
                    .filter(|inner| !reached.contains(inner) && member.declares(inner)),
            );
        }
    }
    Ok(reached)
}

fn sample(trees: &[TokenTree]) -> Vec<String> {
    let mut path: Vec<String> = Vec::new();
    let mut rest: &[TokenTree] = trees;
    while let [TokenTree::Ident(name), after @ ..] = rest {
        path.push(name.to_string());
        rest = match after {
            [first, second, more @ ..] if is_punct(first, ':') && is_punct(second, ':') => more,
            _ => &[],
        };
    }
    path
}

/// The path that opens each `pinned(&…` in the stream.
pub fn pinned(stream: &TokenStream) -> Vec<Vec<String>> {
    let mut found: Vec<Vec<String>> = Vec::new();
    levels(stream, &mut |trees| {
        for pair in trees.windows(2) {
            if let (true, TokenTree::Group(group)) = (is_ident(&pair[0], "pinned"), &pair[1]) {
                let passed: Vec<TokenTree> = group.stream().into_iter().collect();
                if group.delimiter() == Delimiter::Parenthesis && passed.first().is_some_and(|first| is_punct(first, '&')) {
                    found.push(sample(passed.get(1..).unwrap_or(&[])));
                }
            }
        }
    });
    found
}

/// The samples a member's stored types need: `Enum::Variant` for each variant, and the name of a struct an impl names.
pub fn wanted(member: &Member) -> Result<Vec<String>, Unread> {
    let direct: BTreeSet<String> = direct(member)?;
    let mut wanted: Vec<String> = Vec::new();
    for name in reached(member, &direct)? {
        match member.declared(&name)? {
            Head::Enum(item) => wanted.extend(item.variants.iter().map(|variant| format!("{name}::{}", variant.ident))),
            Head::Struct(_) if direct.contains(&name) => wanted.push(name),
            Head::Struct(_) => {}
        }
    }
    Ok(wanted)
}

pub fn unpinned(wanted: &[String], pinned: &[Vec<String>]) -> Vec<String> {
    wanted
        .iter()
        .filter(|sample| {
            !pinned
                .iter()
                .any(|path| path.join("::") == **sample || path.join("::").starts_with(&format!("{sample}::")))
        })
        .cloned()
        .collect()
}

pub fn stored(_asked: &[String]) -> Result<Verdict, Unread> {
    let workspace: Workspace = Workspace::read()?;
    let mut found: Vec<String> = Vec::new();
    let mut pins: usize = 0;
    for package in &workspace.packages {
        let wanted: Vec<String> = wanted(&Member::read(&workspace, package)?)?;
        let test: PathBuf = package.folder().join("tests").join("stored_encoding.rs");
        let shown: String = workspace.relative(&test);
        let held: Vec<Vec<String>> = if test.exists() {
            pinned(&Source::read(&test, shown.clone())?.tokens)
        } else {
            Vec::new()
        };
        pins += wanted.len();
        found.extend(
            unpinned(&wanted, &held)
                .iter()
                .map(|sample| format!("arch-btw1: {shown} does not pin {sample}")),
        );
    }
    if pins == 0 {
        return Err(Unread(String::from("no stored type found; the check has nothing to read")));
    }
    Ok(Verdict {
        found,
        fix: vec![
            String::from("A stored type's encoding only grows, so each variant's JSON is pinned where a change to it fails."),
            String::from("Fix: add `pinned(&Type::Variant, form)` to the test, and the variant's JSON to `form`'s match."),
        ],
    })
}
