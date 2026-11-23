use syn::ItemImpl;

use crate::declared::{impls, slot, Member};
use crate::tokens::{followed, Source};
use crate::workspace::Workspace;
use crate::Unread;

pub struct Found {
    pub member: usize,
    pub file: usize,
    pub state: String,
    pub event: String,
    pub item: ItemImpl,
}

/// Every `impl Lifecycle for State { type Event = Name; … }` in the workspace's `src`.
pub struct Lifecycles {
    pub members: Vec<Member>,
    pub found: Vec<Found>,
}

impl Lifecycles {
    pub fn read(workspace: &Workspace) -> Result<Lifecycles, Unread> {
        let mut members: Vec<Member> = Vec::new();
        for package in &workspace.packages {
            members.push(Member::read(workspace, package)?);
        }
        Lifecycles::of(members)
    }

    pub fn of(members: Vec<Member>) -> Result<Lifecycles, Unread> {
        let mut found: Vec<Found> = Vec::new();
        let mut declared: usize = 0;
        for (member, held) in members.iter().enumerate() {
            for (file, (source, parsed)) in held.files.iter().enumerate() {
                declared += followed(&source.tokens, "Lifecycle", "for");
                found.extend(impls(parsed, &["Lifecycle"]).into_iter().filter_map(|read| {
                    slot(read.item, "Event").map(|event| Found {
                        member,
                        file,
                        state: read.holder,
                        event,
                        item: read.item.clone(),
                    })
                }));
            }
        }
        if declared != found.len() || found.is_empty() {
            return Err(Unread(format!(
                "{declared} `Lifecycle for` in the workspace's src, {} read as `impl Lifecycle for State {{ type Event = Name; … }}`",
                found.len()
            )));
        }
        Ok(Lifecycles { members, found })
    }

    pub fn source(&self, found: &Found) -> Option<&Source> {
        self.members
            .get(found.member)
            .and_then(|member| member.files.get(found.file))
            .map(|file| &file.0)
    }

    pub fn member(&self, found: &Found) -> Result<&Member, Unread> {
        self.members
            .get(found.member)
            .ok_or_else(|| Unread(format!("{}: its crate was not read", found.state)))
    }
}
