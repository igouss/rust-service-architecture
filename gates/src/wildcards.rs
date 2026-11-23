use proc_macro2::Span;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::visit::{self, Visit};
use syn::{Expr, ExprMatch, Macro, Pat, Token};

use crate::tokens::{fat_arrows, squeezed, Source};
use crate::workspace::Workspace;
use crate::{Unread, Verdict};

const SHELL_FREE: [&str; 3] = ["domain", "kernel", "usecase"];

#[derive(Default)]
struct Arms {
    read: usize,
    loose: Vec<Span>,
}

fn bare(pattern: &Pat) -> bool {
    let Pat::Ident(named) = pattern else {
        return false;
    };
    named.subpat.is_none() && named.ident.to_string().starts_with(|first: char| first.is_ascii_lowercase() || first == '_')
}

/// The pattern without the `if` that guards it.
pub fn unguarded(pattern: &Pat) -> &Pat {
    if let Pat::Guard(guarded) = pattern {
        return unguarded(&guarded.pat);
    }
    pattern
}

fn loose(pattern: &Pat, element: bool) -> bool {
    if let Pat::Guard(guarded) = pattern {
        return loose(&guarded.pat, element);
    }
    if let Pat::Or(either) = pattern {
        return either.cases.iter().any(|case| loose(case, element));
    }
    if let Pat::Reference(reference) = pattern {
        return loose(&reference.pat, element);
    }
    if let Pat::Ident(syn::PatIdent { subpat: Some(bound), .. }) = pattern {
        return loose(&bound.1, element);
    }
    if let Pat::Paren(inner) = pattern {
        return loose(&inner.pat, true);
    }
    if let Pat::Tuple(tuple) = pattern {
        return tuple.elems.iter().any(|inner| loose(inner, true));
    }
    element && (matches!(pattern, Pat::Wild(_) | Pat::Rest(_)) || bare(pattern))
}

fn alternatives(pattern: &Pat) -> Vec<&Pat> {
    if let Pat::Or(either) = unguarded(pattern) {
        return either.cases.iter().map(unguarded).collect();
    }
    vec![unguarded(pattern)]
}

impl<'ast> Visit<'ast> for Arms {
    fn visit_expr_match(&mut self, node: &'ast ExprMatch) {
        for arm in &node.arms {
            self.read += 1;
            self.loose.extend(
                alternatives(&arm.pat)
                    .into_iter()
                    .filter(|alternative| loose(alternative, false))
                    .map(Spanned::span),
            );
        }
        visit::visit_expr_match(self, node);
    }

    fn visit_macro(&mut self, node: &'ast Macro) {
        if let Ok(passed) = node.parse_body_with(Punctuated::<Expr, Token![,]>::parse_terminated) {
            passed.iter().for_each(|expression| Arms::visit_expr(self, expression));
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Read {
    pub arms: usize,
    pub loose: Vec<String>,
}

pub fn read(source: &Source) -> Result<Read, Unread> {
    let parsed: syn::File = source.parsed()?;
    let mut arms: Arms = Arms::default();
    arms.visit_file(&parsed);
    let arrows: usize = fat_arrows(&source.tokens);
    if arms.read != arrows {
        return Err(Unread(format!("{}: {arrows} `=>` and {} match arms read", source.shown, arms.read)));
    }
    Ok(Read {
        arms: arms.read,
        loose: arms.loose.iter().map(|span| squeezed(source.slice(*span))).collect(),
    })
}

pub fn wildcards(_asked: &[String]) -> Result<Verdict, Unread> {
    let workspace: Workspace = Workspace::read()?;
    let mut found: Vec<String> = Vec::new();
    let mut arms: usize = 0;
    for package in workspace.packages.iter().filter(|package| package.has_role(&SHELL_FREE)) {
        for path in workspace.rust_files(package, &["src"])? {
            let source: Source = Source::read(&path, workspace.relative(&path))?;
            let read: Read = read(&source)?;
            arms += read.arms;
            found.extend(read.loose.iter().map(|pattern| format!("arch-j0k: {} `{pattern}`", source.shown)));
        }
    }
    if arms == 0 {
        return Err(Unread(format!(
            "no match arm in a crate of role {}; the check has nothing to read",
            SHELL_FREE.join(", ")
        )));
    }
    Ok(Verdict {
        found,
        fix: vec![
            String::from("A tuple element that is `_`, `..` or a bare name takes every case the arms above left, and a new variant lands there unseen."),
            String::from("Fix: spell out every pair, naming each variant the element can be (`A | B`)."),
        ],
    })
}
