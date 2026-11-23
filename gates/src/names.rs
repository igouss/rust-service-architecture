use std::collections::BTreeSet;

use syn::spanned::Spanned;
use syn::{Expr, ImplItem, Lit};

use crate::declared::impls;
use crate::tokens::{followed, Source};
use crate::workspace::Workspace;
use crate::{Unread, Verdict};

const KINDS: [&str; 2] = ["Lifecycle", "Projection"];

#[derive(Debug, PartialEq, Eq)]
pub struct Named {
    pub kind: String,
    pub name: String,
    pub holder: String,
}

fn literal(source: &Source, constant: &syn::ImplItemConst) -> Option<String> {
    let Expr::Lit(syn::ExprLit { lit: Lit::Str(text), .. }) = &constant.expr else {
        return None;
    };
    let written: String = text.token().to_string();
    let typed: String = source.slice(constant.ty.span()).split_whitespace().collect();
    (constant.ident == "NAME" && typed == "&'staticstr" && written.starts_with('"') && !written.contains('\\')).then(|| text.value())
}

/// The `NAME` of every lifecycle and projection the file implements.
pub fn named(source: &Source, file: &syn::File) -> Result<Vec<Named>, Unread> {
    let mut found: Vec<Named> = Vec::new();
    for held in impls(file, &KINDS) {
        let names: Vec<String> = held
            .item
            .items
            .iter()
            .filter_map(|inner| {
                if let ImplItem::Const(constant) = inner {
                    literal(source, constant)
                } else {
                    None
                }
            })
            .collect();
        let [name] = names.as_slice() else {
            return Err(Unread(format!(
                "{}: impl {} for {} holds {} NAME written as a string literal; the scan needs exactly one",
                source.shown,
                held.kind,
                held.holder,
                names.len()
            )));
        };
        found.push(Named {
            kind: held.kind,
            name: name.clone(),
            holder: format!("{} ({})", held.holder, source.shown),
        });
    }
    Ok(found)
}

pub fn shared(found: &[Named]) -> Vec<String> {
    let keys: BTreeSet<(&str, &str)> = found.iter().map(|one| (one.kind.as_str(), one.name.as_str())).collect();
    let mut lines: Vec<String> = Vec::new();
    for (kind, name) in keys {
        let holders: Vec<&str> = found
            .iter()
            .filter(|one| one.kind == kind && one.name == name)
            .map(|one| one.holder.as_str())
            .collect();
        let kinds: &str = if kind == "Lifecycle" { "lifecycles" } else { "projections" };
        if holders.len() > 1 {
            lines.push(format!("arch-1bmf: two {kinds} are named \"{name}\": {}", holders.join(" and ")));
        }
    }
    lines
}

fn read() -> Result<Vec<Named>, Unread> {
    let workspace: Workspace = Workspace::read()?;
    let mut found: Vec<Named> = Vec::new();
    let mut declared: usize = 0;
    for package in &workspace.packages {
        for path in workspace.rust_files(package, &["src"])? {
            let source: Source = Source::read(&path, workspace.relative(&path))?;
            declared += KINDS.iter().map(|kind| followed(&source.tokens, kind, "for")).sum::<usize>();
            found.append(&mut named(&source, &source.parsed()?)?);
        }
    }
    if declared != found.len() || found.is_empty() {
        return Err(Unread(format!(
            "{declared} `Lifecycle for` and `Projection for` in the workspace's src, {} read as `impl … for Type {{ const NAME … }}`",
            found.len()
        )));
    }
    Ok(found)
}

pub fn names(_asked: &[String]) -> Result<Verdict, Unread> {
    let found: Vec<Named> = read().map_err(|unread| Unread(format!("{}; the check would pass it unread", unread.0)))?;
    Ok(Verdict {
        found: shared(&found),
        fix: vec![
            String::from("A NAME names the stored rows; two types with one name read and overwrite each other's rows."),
            String::from("Fix: give each its own NAME. A NAME that has been stored is never reused for another type."),
        ],
    })
}
