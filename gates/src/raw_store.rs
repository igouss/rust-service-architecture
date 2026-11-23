use crate::workspace::{Package, Workspace};
use crate::{Unread, Verdict};

const BANNED: [&str; 2] = ["serde", "serde_json"];

fn adapters(workspace: &Workspace) -> Vec<&Package> {
    workspace.packages.iter().filter(|package| package.has_role(&["driven-adapter"])).collect()
}

/// Each driven adapter with a dependency, outside its tests, on a serializer or on the core.
pub fn forbidden(workspace: &Workspace) -> Vec<(String, String)> {
    let core = |name: &str| {
        workspace
            .packages
            .iter()
            .any(|package| package.name == name && package.has_role(&["domain", "kernel"]))
    };
    adapters(workspace)
        .iter()
        .flat_map(|adapter| adapter.dependencies.iter().map(move |dependency| (adapter, dependency)))
        .filter(|held| held.1.kind.as_deref() != Some("dev") && (BANNED.contains(&held.1.name.as_str()) || core(&held.1.name)))
        .map(|held| (held.0.name.clone(), held.1.name.clone()))
        .collect()
}

pub fn raw_store(_asked: &[String]) -> Result<Verdict, Unread> {
    let workspace: Workspace = Workspace::read()?;
    let mut lines: Vec<String> = forbidden(&workspace)
        .iter()
        .map(|held| format!("raw-store: {} depends on {}", held.0, held.1))
        .collect();
    if !lines.is_empty() {
        lines.push(String::from(
            "A store adapter implements ports::store over text and never names a domain type or serializes.",
        ));
        lines.push(String::from(
            "Fix: move the typed code into ports, or make the dependency a dev-dependency if only tests use it.",
        ));
    }
    if adapters(&workspace).is_empty() {
        lines.push(String::from("raw-store: no driven-adapter crate found; the check has nothing to read"));
    }
    Ok(Verdict { found: lines, fix: Vec::new() })
}
