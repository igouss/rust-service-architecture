use std::path::PathBuf;

use regex::Regex;

use crate::workspace::Workspace;
use crate::{Unread, Verdict};

fn pattern(text: &str) -> Result<Regex, Unread> {
    Regex::new(text).map_err(|error| Unread(format!("the pattern {text} does not compile: {error}")))
}

pub fn unpinned(workflows: &[(String, String)]) -> Result<Vec<String>, Unread> {
    let install: Regex = pattern(r"\bcargo\s+install\b")?;
    let git: Regex = pattern(r"--git(?:[^\w-]|$)")?;
    let pinned: Regex = pattern(r"--rev[= ][0-9a-fA-F]{40}(?:\W|$)")?;
    let named: Regex = pattern(r"--git(?:[= ](\S+))?")?;
    let mut found: Vec<String> = Vec::new();
    let (mut read, mut every, mut sources, mut read_sources): (usize, usize, usize, usize) = (0, 0, 0, 0);
    for (shown, text) in workflows {
        every += install.find_iter(text).count();
        sources += git.find_iter(text).count();
        for (index, line) in text.split('\n').enumerate() {
            let number: usize = index + 1;
            let installs: Vec<usize> = install.find_iter(line).map(|one| one.start()).collect();
            if installs.len() > 1 {
                return Err(Unread(format!(
                    "{shown}:{number}: {} `cargo install` on one line; one `--rev` cannot be read as pinning both",
                    installs.len()
                )));
            }
            let Some(start) = installs.first() else {
                continue;
            };
            read += 1;
            let command: &str = line.get(*start..).unwrap_or("").trim();
            if command.ends_with('\\') {
                return Err(Unread(format!(
                    "{shown}:{number}: `{command}` goes on past the line; write the install on one line"
                )));
            }
            read_sources += git.find_iter(command).count();
            if let (Some(source), false) = (named.captures(command), pinned.is_match(command)) {
                let url: &str = source.get(1).map_or("with no url", |url| url.as_str());
                found.push(format!("arch-mov6: {shown}:{number} cargo install --git {url} names no --rev of 40 hex digits"));
            }
        }
    }
    if read != every {
        return Err(Unread(format!("{every} `cargo install` in the workflows, {read} read one to a line")));
    }
    if read_sources != sources {
        return Err(Unread(format!(
            "{sources} `--git` in the workflows, {read_sources} on the line of a `cargo install`; write each install on one line"
        )));
    }
    Ok(found)
}

pub fn pins(_asked: &[String]) -> Result<Verdict, Unread> {
    let workspace: Workspace = Workspace::read()?;
    let folder: PathBuf = workspace.root.join(".github").join("workflows");
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&folder)
        .map(|entries| entries.filter_map(Result::ok).map(|entry| entry.path()).collect())
        .unwrap_or_default();
    paths.retain(|path| path.extension().is_some_and(|extension| extension == "yml" || extension == "yaml"));
    paths.sort();
    if paths.is_empty() {
        return Err(Unread(format!(
            "no workflow under {}; the check has nothing to read",
            workspace.relative(&folder)
        )));
    }
    let mut workflows: Vec<(String, String)> = Vec::new();
    for path in &paths {
        let shown: String = workspace.relative(path);
        let text: String = std::fs::read_to_string(path).map_err(|error| Unread(format!("{shown}: {error}")))?;
        workflows.push((shown, text));
    }
    Ok(Verdict {
        found: unpinned(&workflows)?,
        fix: vec![
            String::from("A tool built from a branch is whatever the branch holds the day CI runs, so a green no longer says which gate passed it."),
            String::from("Fix: add `--rev <commit>`, the 40-hex id `git ls-remote <url> HEAD` prints, once `just check` passes with that build (docs/adr/0017-gate-tools-are-pinned.md)."),
        ],
    })
}
