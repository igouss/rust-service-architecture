//! The gates of `just check` that read this workspace's own source and manifests.

pub mod allows;
pub mod comments;
pub mod declared;
pub mod docs;
pub mod events;
pub mod lifecycles;
pub mod names;
pub mod pins;
pub mod raw_store;
pub mod refined;
pub mod stored;
pub mod tokens;
pub mod traits;
pub mod views;
pub mod wildcards;
pub mod workspace;

/// A gate that could not read what it checks: red, exit 2, never a pass.
#[derive(Debug, PartialEq, Eq)]
pub struct Unread(pub String);

#[derive(Debug, PartialEq, Eq)]
pub struct Verdict {
    pub found: Vec<String>,
    pub fix: Vec<String>,
}
