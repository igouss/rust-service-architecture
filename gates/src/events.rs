use crate::lifecycles::Lifecycles;
use crate::workspace::Workspace;
use crate::{Unread, Verdict};

const GENERIC: [&str; 8] = ["Updated", "Changed", "Modified", "Edited", "Saved", "Set", "Replaced", "Patched"];

/// Each event variant named for a generic change, as `path: Event::Variant`.
pub fn generic(lifecycles: &Lifecycles) -> Result<Vec<String>, Unread> {
    let mut found: Vec<String> = Vec::new();
    for lifecycle in &lifecycles.found {
        let shown: &str = lifecycles.source(lifecycle).map_or("", |source| source.shown.as_str());
        for variant in lifecycles.member(lifecycle)?.variants(&lifecycle.event)? {
            let bare: &str = variant.strip_prefix(lifecycle.state.as_str()).unwrap_or(&variant);
            if GENERIC.contains(&variant.as_str()) || GENERIC.contains(&bare) {
                found.push(format!("{shown}: {}::{variant}", lifecycle.event));
            }
        }
    }
    Ok(found)
}

pub fn events(_asked: &[String]) -> Result<Verdict, Unread> {
    let found: Vec<String> = Workspace::read()
        .and_then(|workspace| Lifecycles::read(&workspace))
        .and_then(|lifecycles| generic(&lifecycles))
        .map_err(|unread| Unread(format!("{}; the check would pass it unread", unread.0)))?;
    Ok(Verdict {
        found: found
            .iter()
            .map(|line| format!("events: {line} says that something changed, not what happened"))
            .collect(),
        fix: vec![
            String::from("An event names one fact in the domain's words, in the past tense, and carries what that fact decided."),
            String::from("Fix: name what happened (Renamed, Rescheduled, AddressCorrected); if several things can happen, they are several events."),
        ],
    })
}
