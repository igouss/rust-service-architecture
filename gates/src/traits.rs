use std::path::PathBuf;

use proc_macro2::TokenTree;

use crate::tokens::{idents, is_ident, levels, Source};
use crate::workspace::{Package, Workspace};
use crate::{Unread, Verdict};

#[derive(Debug, PartialEq, Eq)]
pub struct Declared {
    pub line: usize,
    pub name: String,
}

pub fn declared(source: &Source) -> Result<Vec<Declared>, Unread> {
    let mut found: Vec<(usize, Declared)> = Vec::new();
    levels(&source.tokens, &mut |trees| {
        for pair in trees.windows(2) {
            if let (true, TokenTree::Ident(name)) = (is_ident(&pair[0], "trait"), &pair[1]) {
                found.push((
                    pair[0].span().byte_range().start,
                    Declared {
                        line: pair[0].span().start().line,
                        name: name.to_string(),
                    },
                ));
            }
        }
    });
    if found.len() != idents(&source.tokens, "trait") {
        return Err(Unread(format!("{}: a `trait` the scan could not name", source.shown)));
    }
    found.sort_by_key(|held| held.0);
    Ok(found.into_iter().map(|held| held.1).collect())
}

fn home(workspace: &Workspace, package: &Package) -> Result<Vec<PathBuf>, Unread> {
    if package.has_role(&["port-and-adapter"]) {
        return workspace.rust_files(package, &["src"]);
    }
    if package.has_role(&["domain", "usecase"]) {
        return Ok(package
            .targets
            .iter()
            .filter(|target| target.kind.iter().any(|kind| kind == "lib"))
            .map(|target| target.src_path.clone())
            .collect());
    }
    Ok(Vec::new())
}

pub fn traits(_asked: &[String]) -> Result<Verdict, Unread> {
    let workspace: Workspace = Workspace::read()?;
    let mut found: Vec<String> = Vec::new();
    for package in &workspace.packages {
        let home: Vec<PathBuf> = home(&workspace, package)?;
        let mut kept: usize = 0;
        for path in workspace.rust_files(package, &[])? {
            let shown: String = workspace.relative(&path);
            let read: Vec<Declared> = declared(&Source::read(&path, shown.clone())?)?;
            if home.contains(&path) {
                kept += read.len();
            } else {
                found.extend(read.iter().map(|one| format!("arch-u8nl: {shown}:{} trait {}", one.line, one.name)));
            }
        }
        if !home.is_empty() && kept == 0 {
            return Err(Unread(format!(
                "{} declares no trait where its primitives live; the check has nothing to read",
                package.name
            )));
        }
    }
    Ok(Verdict {
        found,
        fix: vec![
            String::from("A trait is a new kind of thing; the kinds are the traits of domain/src/lib.rs, app/src/lib.rs and ports/src."),
            String::from("Fix: make the feature an instance of one of them: a lifecycle, a command, a reaction, a projection, a policy, a query, a port."),
        ],
    })
}
