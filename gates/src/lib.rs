//! The gates of `just check` that read this workspace's own source and manifests.

pub mod tokens;
pub mod workspace;

/// A gate that could not read what it checks: red, exit 2, never a pass.
#[derive(Debug, PartialEq, Eq)]
pub struct Unread(pub String);

#[derive(Debug, PartialEq, Eq)]
pub struct Verdict {
    pub found: Vec<String>,
    pub fix: Vec<String>,
}
