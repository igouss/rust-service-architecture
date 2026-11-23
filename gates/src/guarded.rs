use std::process::{Command, Output};

use toml::{Table, Value};

use crate::{Unread, Verdict};

const TABLES: [&[&str]; 3] = [&["workspace", "lints"], &["lints"], &["package", "metadata", "hex-arch"]];
const TOP: [&str; 3] = ["justfile", "clippy.toml", "rustfmt.toml"];
const SCRIPTS: [&str; 2] = [".py", ".bb"];
const UNDER: [&str; 3] = ["planted/", ".github/workflows/", "gates/"];

fn git(asked: &[&str]) -> Result<String, Unread> {
    let done: Output = Command::new("git")
        .args(asked)
        .output()
        .map_err(|error| Unread(format!("git did not run: {error}")))?;
    if !done.status.success() {
        return Err(Unread(format!("git {}: {}", asked.join(" "), String::from_utf8_lossy(&done.stderr).trim())));
    }
    Ok(String::from_utf8_lossy(&done.stdout).into_owned())
}

/// The tables of a manifest that hold a gate: the lints and the `hex-arch` tags.
pub fn tables(manifest: &str) -> Result<Vec<Value>, Unread> {
    let read: Table = manifest.parse().map_err(|error| Unread(format!("a manifest is not TOML: {error}")))?;
    let empty = || Value::Table(Table::new());
    Ok(TABLES
        .iter()
        .map(|keys| {
            keys.iter().fold(Value::Table(read.clone()), |value, key| {
                value.as_table().and_then(|table| table.get(*key)).cloned().unwrap_or_else(empty)
            })
        })
        .collect())
}

fn held(commit: &str, path: &str) -> Result<String, Unread> {
    let named: String = format!("{commit}:{path}");
    let there: bool = Command::new("git")
        .args(["cat-file", "-e", &named])
        .output()
        .is_ok_and(|done| done.status.success());
    if there {
        git(&["show", &named])
    } else {
        Ok(String::new())
    }
}

fn manifest(path: &str) -> bool {
    path.rsplit('/').next() == Some("Cargo.toml")
}

/// Whether a changed path, a manifest apart, is one of the files that hold a gate.
pub fn listed(path: &str) -> bool {
    let top: bool = !path.contains('/');
    (top && (TOP.contains(&path) || SCRIPTS.iter().any(|ending| path.ends_with(ending))))
        || UNDER.iter().any(|folder| path.starts_with(folder))
        || path.rsplit('/').next() == Some("clippy.toml")
}

fn gate(base: &str, path: &str) -> Result<bool, Unread> {
    if manifest(path) {
        return Ok(tables(&held(base, path)?)? != tables(&held("HEAD", path)?)?);
    }
    Ok(listed(path))
}

pub fn guarded(asked: &[String]) -> Result<Verdict, Unread> {
    let [named] = asked else {
        return Err(Unread(String::from("name the base, as in `just guarded origin/main`")));
    };
    let base: String = git(&["merge-base", named, "HEAD"])?.trim().to_string();
    let short: &str = base.get(..7).unwrap_or(&base);
    let listing: String = git(&["diff", "--name-only", "--no-renames", &base, "HEAD"])?;
    let changed: Vec<&str> = listing.split('\n').filter(|path| !path.is_empty()).collect();
    let mut gates: Vec<&str> = Vec::new();
    for path in &changed {
        if gate(&base, path)? {
            gates.push(path);
        }
    }
    let records: usize = git(&["diff", "--name-only", "--no-renames", "--diff-filter=AM", &base, "HEAD", "--", "docs/adr/"])?
        .split_whitespace()
        .count();
    if gates.is_empty() || records != 0 {
        println!(
            "guarded: {} file(s) changed since {short}, {} of them a gate, {records} decision record(s)",
            changed.len(),
            gates.len()
        );
        return Ok(Verdict {
            found: Vec::new(),
            fix: Vec::new(),
        });
    }
    Ok(Verdict {
        found: gates
            .iter()
            .map(|path| format!("arch-tsu: {path} changes a gate and the range {short}..HEAD holds no decision record"))
            .collect(),
        fix: vec![
            String::from("A gate is weakened where nobody looks unless the change is its own decision."),
            String::from("Fix: add or amend a file under docs/adr/ in this range: what the gate held, what it holds now, and what would reverse it."),
        ],
    })
}
