use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use regex::Regex;

use crate::planted::{read, Expect, Mutant};
use crate::workspace::Workspace;
use crate::{Unread, Verdict};

fn unread(path: &Path) -> impl Fn(std::io::Error) -> Unread + '_ {
    move |error| Unread(format!("{}: {error}", path.display()))
}

fn copy(from: &Path, to: &Path) -> Result<(), Unread> {
    if !from.is_dir() {
        return fs::copy(from, to).map(|_| ()).map_err(unread(from));
    }
    fs::create_dir_all(to).map_err(unread(to))?;
    for entry in fs::read_dir(from).map_err(unread(from))? {
        let inner: PathBuf = entry.map_err(unread(from))?.path();
        if let Some(name) = inner.file_name() {
            copy(&inner, &to.join(name))?;
        }
    }
    Ok(())
}

fn every(pattern: &str, output: &str) -> Result<Vec<String>, Unread> {
    let read: Regex = Regex::new(pattern).map_err(|error| Unread(format!("the pattern {pattern} does not compile: {error}")))?;
    let found: BTreeSet<String> = read
        .captures_iter(output)
        .filter_map(|found| found.get(1))
        .map(|found| found.as_str().to_string())
        .collect();
    Ok(found.into_iter().collect())
}

/// Whether a run killed the mutant the way it was expected to, and the line that says how.
pub fn judged(expect: &Expect, reachable: bool, code: i32, output: &str) -> Result<(bool, String), Unread> {
    let failed: Vec<String> = every(r"(?m)^---- (\S+) stdout ----", output)?;
    let codes: Vec<String> = every(r"error\[(E\d+)\]", output)?;
    let uncompiled: String = format!(
        "DID NOT COMPILE: {}",
        codes.iter().map(|code| format!("error[{code}]")).collect::<Vec<String>>().join(" ")
    );
    let killed: String = format!("killed by {}", failed.join(", "));
    Ok(match expect {
        Expect::Server { needs, .. } if !reachable => (false, format!("NEEDS {needs}: this mutant is killed by tests that run against a server")),
        Expect::Server { .. } if output.contains("could not compile") => (false, uncompiled),
        Expect::Server { .. } if every(r"test result: \w+\. ([1-9]\d*) passed", output)?.is_empty() => {
            (false, String::from("NO TEST PASSED: the server is not reachable, so a failure proves nothing"))
        }
        Expect::Server { .. } => (
            code != 0 && !failed.is_empty(),
            if failed.is_empty() { String::from("SURVIVED") } else { killed },
        ),
        Expect::Gate { names, .. } => {
            let named: &str = if output.contains(names.as_str()) { "names" } else { "DOES NOT NAME" };
            (code != 0 && output.contains(names.as_str()), format!("exit {code}, {named} {names}"))
        }
        Expect::Kind(_) if code == 0 => (false, String::from("SURVIVED")),
        Expect::Kind(kind) if kind == "test" && output.contains("could not compile") => (false, uncompiled),
        Expect::Kind(kind) if kind == "test" => (!failed.is_empty(), killed),
        Expect::Kind(kind) => {
            let unused: bool = output.contains("unused") && output.contains("must be used");
            (
                if kind == "unused" { unused } else { codes.contains(kind) },
                format!("{}{}", if unused { "unused_must_use" } else { "" }, codes.join(" ")),
            )
        }
    })
}

fn command(expect: &Expect) -> Vec<String> {
    let words = |text: &str| text.split(' ').map(String::from).collect::<Vec<String>>();
    match expect {
        Expect::Kind(kind) if kind == "test" => words("cargo test -q --no-fail-fast"),
        Expect::Kind(_) => words("cargo check -q --all-targets"),
        Expect::Gate { gate, .. } => gate.clone(),
        Expect::Server { server, .. } => server.clone(),
    }
}

fn label(expect: &Expect) -> String {
    match expect {
        Expect::Kind(kind) => kind.clone(),
        Expect::Gate { gate: run, .. } | Expect::Server { server: run, .. } => run.iter().take(2).cloned().collect::<Vec<String>>().join(" "),
    }
}

fn scratch(workspace: &Workspace, mutant: &Mutant, into: &Path) -> Result<Option<usize>, Unread> {
    for entry in fs::read_dir(&workspace.root).map_err(unread(&workspace.root))? {
        let path: PathBuf = entry.map_err(unread(&workspace.root))?.path();
        let hidden: bool = path.file_name().is_some_and(|name| name.to_string_lossy().starts_with('.'));
        if let (true, false, Some(name)) = (path.is_file(), hidden, path.file_name()) {
            copy(&path, &into.join(name))?;
        }
    }
    let mut folders: Vec<PathBuf> = workspace.packages.iter().map(|package| package.folder().to_path_buf()).collect();
    folders.extend([workspace.root.join("planted"), workspace.root.join(".github")]);
    if matches!(mutant.expect, Expect::Gate { .. }) {
        folders.push(workspace.root.join("docs"));
    }
    for folder in folders.iter().filter(|folder| **folder != workspace.root) {
        copy(folder, &into.join(workspace.relative(folder)))?;
    }
    let target: PathBuf = into.join(&mutant.path);
    let source: String = fs::read_to_string(&target).map_err(unread(&target))?;
    let anchors: usize = source.matches(mutant.anchor.as_str()).count();
    if anchors != 1 {
        return Ok(Some(anchors));
    }
    fs::write(&target, source.replace(mutant.anchor.as_str(), &mutant.replacement)).map_err(unread(&target))?;
    Ok(None)
}

fn planted(workspace: &Workspace, mutant: &Mutant, into: &Path) -> Result<(bool, String), Unread> {
    if let Some(anchors) = scratch(workspace, mutant, into)? {
        return Ok((false, format!("ANCHOR FOUND {anchors} TIMES")));
    }
    let reachable: bool = if let Expect::Server { needs, .. } = &mutant.expect {
        std::env::var(needs).is_ok_and(|set| !set.is_empty())
    } else {
        true
    };
    if !reachable {
        return judged(&mutant.expect, false, 0, "");
    }
    let run: Vec<String> = command(&mutant.expect);
    let (program, passed): (&String, &[String]) = run.split_first().ok_or_else(|| Unread(format!("{}: its gate names no command", mutant.name)))?;
    let done: Output = Command::new(program)
        .args(passed)
        .current_dir(into)
        .env("CARGO_TARGET_DIR", workspace.root.join("target").join("mutants"))
        .output()
        .map_err(|error| Unread(format!("{program} did not run: {error}")))?;
    let output: String = format!("{}{}", String::from_utf8_lossy(&done.stdout), String::from_utf8_lossy(&done.stderr));
    judged(&mutant.expect, true, done.status.code().unwrap_or(-1), &output)
}

pub fn listed(mutants: &[Mutant]) -> Vec<String> {
    mutants
        .iter()
        .map(|mutant| {
            format!(
                "{:<9} {:<28} {}",
                if mutant.example { "example" } else { "machinery" },
                mutant.path,
                mutant.name
            )
        })
        .collect()
}

pub fn mutants(asked: &[String]) -> Result<Verdict, Unread> {
    let workspace: Workspace = Workspace::read()?;
    let all: Vec<Mutant> = read(&workspace.root)?;
    if asked.iter().map(String::as_str).eq(["--list"]) {
        listed(&all).iter().for_each(|line| println!("{line}"));
        return Ok(Verdict {
            found: Vec::new(),
            fix: Vec::new(),
        });
    }
    let mut red: usize = 0;
    for (index, mutant) in all
        .iter()
        .enumerate()
        .filter(|held| asked.is_empty() || asked.iter().any(|id| id == held.1.id()))
    {
        let into: PathBuf = std::env::temp_dir().join(format!("mutant-{}-{index}", std::process::id()));
        fs::create_dir_all(&into).map_err(unread(&into))?;
        let outcome: Result<(bool, String), Unread> = planted(&workspace, mutant, &into);
        fs::remove_dir_all(&into).map_err(unread(&into))?;
        let (killed, detail): (bool, String) = outcome?;
        red += usize::from(!killed);
        println!("{:<4} {} [{}] {detail}", if killed { "ok" } else { "FAIL" }, mutant.name, label(&mutant.expect));
    }
    let summary: String = format!("{red} mutant(s) not killed as expected");
    if red == 0 {
        println!("{summary}");
        return Ok(Verdict {
            found: Vec::new(),
            fix: Vec::new(),
        });
    }
    Ok(Verdict {
        found: vec![summary],
        fix: Vec::new(),
    })
}
