use std::path::{Component, Path, PathBuf};
use std::process::{Command, Output};

use proc_macro2::{Delimiter, TokenStream, TokenTree};
use regex::Regex;

use crate::tokens::{is_ident, is_punct, levels, Source};
use crate::workspace::{Package, Workspace};
use crate::{Unread, Verdict};

const BANNED: [&str; 2] = ["serde", "serde_json"];
const SUITE: &str = "include!(\"../../ports/tests/conformance/suite.rs\");";
const SUITE_FOLDER: &str = "../../ports/tests/conformance";

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

/// How many `#[test]` the stream holds.
pub fn cases(stream: &TokenStream) -> usize {
    let mut count: usize = 0;
    levels(stream, &mut |trees| {
        count += trees
            .windows(2)
            .filter(|pair| {
                let named: Vec<TokenTree> = if let TokenTree::Group(group) = &pair[1] {
                    if group.delimiter() == Delimiter::Bracket {
                        group.stream().into_iter().collect()
                    } else {
                        Vec::new()
                    }
                } else {
                    Vec::new()
                };
                is_punct(&pair[0], '#') && named.len() == 1 && named.first().is_some_and(|only| is_ident(only, "test"))
            })
            .count();
    });
    count
}

/// The tests a `cargo test` run reports as passed and as failed.
pub fn tally(output: &str) -> Result<(usize, usize), Unread> {
    let count = |pattern: &str| -> Result<usize, Unread> {
        let read: Regex = Regex::new(pattern).map_err(|error| Unread(format!("the pattern {pattern} does not compile: {error}")))?;
        Ok(read
            .captures_iter(output)
            .filter_map(|found| found.get(1))
            .filter_map(|digits| digits.as_str().parse::<usize>().ok())
            .sum())
    };
    Ok((count(r"test result: \w+\. (\d+) passed")?, count(r"test result: [^\n]*; (\d+) failed")?))
}

fn normal(path: &Path) -> PathBuf {
    path.components().fold(PathBuf::new(), |mut built, part| {
        if part == Component::ParentDir {
            built.pop();
        } else {
            built.push(part);
        }
        built
    })
}

fn ran(workspace: &Workspace, name: &str, feature: &str) -> Result<Verdict, Unread> {
    let adapter: &Package = adapters(workspace)
        .into_iter()
        .find(|adapter| adapter.name == name)
        .ok_or_else(|| Unread(format!("no driven-adapter crate is called {name}")))?;
    let suite: PathBuf = normal(&adapter.folder().join("tests").join(SUITE_FOLDER));
    let mut written: usize = 0;
    for path in workspace.rust_files_in(&suite)? {
        written += cases(&Source::read(&path, workspace.relative(&path))?.tokens);
    }
    let done: Output = Command::new("cargo")
        .args(["test", "-q", "-p", name, "--features", feature, "--test", "conformance"])
        .output()
        .map_err(|error| Unread(format!("cargo test did not run: {error}")))?;
    let output: String = format!("{}{}", String::from_utf8_lossy(&done.stdout), String::from_utf8_lossy(&done.stderr));
    let (passed, failed): (usize, usize) = tally(&output)?;
    println!(
        "raw-store: {name} ran {} of the {written} conformance cases and passed {passed}",
        passed + failed
    );
    if done.status.success() && passed == written && written != 0 {
        return Ok(Verdict {
            found: Vec::new(),
            fix: Vec::new(),
        });
    }
    Ok(Verdict {
        found: vec![
            output,
            String::from("Fix: every case of the suite runs and passes against this store; a case that is filtered, ignored or behind an unset feature has proved nothing."),
        ],
        fix: Vec::new(),
    })
}

fn unproved(workspace: &Workspace, adapter: &Package) -> Result<bool, Unread> {
    let mut source: String = String::new();
    for path in workspace.rust_files(adapter, &["src"])? {
        source.push_str(&std::fs::read_to_string(&path).map_err(|error| Unread(format!("{}: {error}", workspace.relative(&path))))?);
    }
    let test: String = std::fs::read_to_string(adapter.folder().join("tests").join("conformance.rs")).unwrap_or_default();
    Ok(source.contains("impl Store for") && !test.contains(SUITE))
}

pub fn raw_store(asked: &[String]) -> Result<Verdict, Unread> {
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
    if let [flag, name, feature] = asked {
        if flag == "--ran" {
            lines.iter().for_each(|line| println!("{line}"));
            return ran(&workspace, name, feature);
        }
    }
    for adapter in adapters(&workspace) {
        if unproved(&workspace, adapter)? {
            lines.push(format!(
                "raw-store: {} implements Store and does not include the conformance suite",
                adapter.name
            ));
            lines.push(format!("Fix: add tests/conformance.rs that names the adapter as Subject and ends with {SUITE}"));
        }
    }
    if adapters(&workspace).is_empty() {
        lines.push(String::from("raw-store: no driven-adapter crate found; the check has nothing to read"));
    }
    Ok(Verdict { found: lines, fix: Vec::new() })
}
