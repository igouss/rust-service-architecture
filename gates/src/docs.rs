use std::collections::BTreeSet;
use std::path::PathBuf;

use regex::Regex;

use crate::workspace::Workspace;
use crate::{Unread, Verdict};

fn pattern(text: &str) -> Result<Regex, Unread> {
    Regex::new(text).map_err(|error| Unread(format!("the pattern {text} does not compile: {error}")))
}

fn distinct(found: impl Iterator<Item = String>) -> Vec<String> {
    let mut seen: BTreeSet<String> = BTreeSet::new();
    found.filter(|one| seen.insert(one.clone())).collect()
}

/// The text with every line trimmed and the blank ones dropped.
pub fn normalized(text: &str) -> String {
    text.lines().map(str::trim).filter(|line| !line.is_empty()).collect::<Vec<&str>>().join("\n")
}

/// A normalized block without the braces that close it, so a block may stop before its item does.
pub fn open_ended(normalized: &str) -> String {
    let lines: Vec<&str> = normalized.lines().collect();
    let kept: usize = lines.iter().rposition(|line| *line != "}").map_or(0, |last| last + 1);
    lines.iter().take(kept).copied().collect::<Vec<&str>>().join("\n")
}

pub fn rust_blocks(text: &str) -> Result<Vec<String>, Unread> {
    Ok(pattern(r"(?s)```rust\n(.*?)```")?
        .captures_iter(text)
        .filter_map(|found| found.get(1))
        .map(|block| block.as_str().to_string())
        .collect())
}

/// The names in backticks that read as a test: five words or more joined by underscores.
pub fn cited_tests(text: &str) -> Result<Vec<String>, Unread> {
    Ok(distinct(
        pattern(r"`([a-z][a-z0-9]*(?:_[a-z0-9]+){4,})`")?
            .captures_iter(text)
            .filter_map(|found| found.get(1))
            .map(|name| name.as_str().to_string()),
    ))
}

pub struct Held {
    pub sources: Vec<String>,
    pub rust: String,
}

/// What is wrong with one document: a block the sources do not hold, or a test that does not exist.
pub fn problems(name: &str, text: &str, held: &Held) -> Result<(Vec<String>, Vec<String>), Unread> {
    let mut missing: Vec<String> = Vec::new();
    for (index, block) in rust_blocks(text)?.iter().enumerate() {
        let wanted: String = open_ended(&normalized(block));
        if !held.sources.iter().any(|source| source.contains(&wanted)) {
            missing.push(format!("{name} rust block {}: {}", index + 1, block.lines().next().unwrap_or("")));
        }
    }
    let mut unknown: Vec<String> = Vec::new();
    for test in cited_tests(text)? {
        if !held.rust.contains(&format!("fn {test}(")) {
            unknown.push(format!("{name} names a test that does not exist: {test}"));
        }
    }
    Ok((missing, unknown))
}

fn held(workspace: &Workspace) -> Result<Held, Unread> {
    let mut sources: Vec<String> = Vec::new();
    let mut rust: Vec<String> = Vec::new();
    for package in &workspace.packages {
        for under in ["src", "tests"] {
            for path in workspace.rust_files(package, &[under])? {
                let text: String = std::fs::read_to_string(&path).map_err(|error| Unread(format!("{}: {error}", workspace.relative(&path))))?;
                if under == "src" {
                    sources.push(normalized(&text));
                }
                rust.push(text);
            }
        }
    }
    Ok(Held {
        sources,
        rust: rust.join("\n"),
    })
}

pub fn docs(_asked: &[String]) -> Result<Verdict, Unread> {
    let workspace: Workspace = Workspace::read()?;
    let folder: PathBuf = workspace.root.join("docs").join("architecture");
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&folder)
        .map_err(|error| Unread(format!("{}: {error}", workspace.relative(&folder))))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();
    paths.retain(|path| path.file_name().is_some_and(|name| name.to_string_lossy().ends_with(".req.md")));
    paths.sort();
    let held: Held = held(&workspace)?;
    let (mut missing, mut unknown, mut total): (Vec<String>, Vec<String>, usize) = (Vec::new(), Vec::new(), 0);
    for path in &paths {
        let name: String = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
        let text: String = std::fs::read_to_string(path).map_err(|error| Unread(format!("{}: {error}", workspace.relative(path))))?;
        let (mut gone, mut wrong): (Vec<String>, Vec<String>) = problems(&name, &text, &held)?;
        total += rust_blocks(&text)?.len();
        missing.append(&mut gone);
        unknown.append(&mut wrong);
    }
    let green: bool = total != 0 && missing.is_empty() && unknown.is_empty();
    let mut lines: Vec<String> = missing.iter().chain(unknown.iter()).cloned().collect();
    lines.push(format!(
        "{total} rust blocks, {} not found in the workspace; {} cited names unknown",
        missing.len(),
        unknown.len()
    ));
    if !missing.is_empty() {
        lines.push(String::from(
            "arch-ajd: a Rust block in docs/architecture must be code in the workspace, verbatim.",
        ));
        lines.push(String::from(
            "Fix: copy the changed code into the block, or replace the block with a pointer to the file.",
        ));
    }
    if !unknown.is_empty() {
        lines.push(String::from("arch-7iz: a criterion must name a test that exists."));
        lines.push(String::from("Fix: rename the citation to the test as it is now called."));
    }
    if green {
        lines.iter().for_each(|line| println!("{line}"));
        return Ok(Verdict {
            found: Vec::new(),
            fix: Vec::new(),
        });
    }
    Ok(Verdict { found: lines, fix: Vec::new() })
}
