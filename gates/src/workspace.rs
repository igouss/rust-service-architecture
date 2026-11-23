use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde::Deserialize;
use serde_json::Value;

use crate::Unread;

#[derive(Deserialize)]
pub struct Target {
    pub kind: Vec<String>,
    pub src_path: PathBuf,
}

#[derive(Deserialize)]
pub struct Dependency {
    pub name: String,
    pub kind: Option<String>,
}

#[derive(Deserialize)]
pub struct Package {
    pub name: String,
    pub manifest_path: PathBuf,
    pub targets: Vec<Target>,
    pub dependencies: Vec<Dependency>,
    #[serde(default)]
    metadata: Value,
}

#[derive(Deserialize)]
pub struct Workspace {
    pub packages: Vec<Package>,
    #[serde(rename = "workspace_root")]
    pub root: PathBuf,
}

impl Package {
    pub fn role(&self) -> Option<&str> {
        self.metadata.get("hex-arch")?.get("role")?.as_str()
    }

    pub fn folder(&self) -> &Path {
        self.manifest_path.parent().unwrap_or(Path::new("."))
    }

    pub fn has_role(&self, roles: &[&str]) -> bool {
        self.role().is_some_and(|role| roles.contains(&role))
    }
}

impl Workspace {
    pub fn read() -> Result<Workspace, Unread> {
        let done: Output = Command::new("cargo")
            .args(["metadata", "--format-version", "1", "--no-deps"])
            .output()
            .map_err(|error| Unread(format!("cargo metadata did not run: {error}")))?;
        if !done.status.success() {
            return Err(Unread(format!("cargo metadata failed: {}", String::from_utf8_lossy(&done.stderr).trim())));
        }
        let mut read: Workspace = serde_json::from_slice(&done.stdout).map_err(|error| Unread(format!("cargo metadata was not read: {error}")))?;
        read.packages.sort_by(|one, other| one.name.cmp(&other.name));
        Ok(read)
    }

    pub fn relative(&self, path: &Path) -> String {
        path.strip_prefix(&self.root).unwrap_or(path).display().to_string()
    }

    pub fn rust_files(&self, package: &Package, under: &[&str]) -> Result<Vec<PathBuf>, Unread> {
        let top: PathBuf = under.iter().fold(package.folder().to_path_buf(), |path, part| path.join(part));
        let mut found: Vec<PathBuf> = Vec::new();
        walk(&top, &mut found)?;
        if !under.is_empty() {
            return Ok(found);
        }
        let missing: Vec<String> = package
            .targets
            .iter()
            .filter(|target| !found.contains(&target.src_path))
            .map(|target| target.src_path.display().to_string())
            .collect();
        if missing.is_empty() && !found.is_empty() {
            return Ok(found);
        }
        let unwalked: String = if missing.is_empty() { String::from("nothing") } else { missing.join(", ") };
        Err(Unread(format!("{}: cargo builds {unwalked} and the scan did not walk it", package.name)))
    }
}

fn walk(folder: &Path, found: &mut Vec<PathBuf>) -> Result<(), Unread> {
    if !folder.is_dir() {
        return Ok(());
    }
    let unlisted = |error: std::io::Error| Unread(format!("{}: {error}", folder.display()));
    let mut files: Vec<PathBuf> = Vec::new();
    let mut folders: Vec<PathBuf> = Vec::new();
    for entry in fs::read_dir(folder).map_err(unlisted)? {
        let path: PathBuf = entry.map_err(unlisted)?.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name != "target") {
                folders.push(path);
            }
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
    files.sort();
    folders.sort();
    found.append(&mut files);
    folders.iter().try_for_each(|inner| walk(inner, found))
}
