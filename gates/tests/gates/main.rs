#![allow(clippy::expect_used, clippy::panic)]

use gates::tokens::Source;

mod allows;
mod comments;
mod docs;
mod events;
mod guarded;
mod mutants;
mod names;
mod pins;
mod raw_store;
mod refined;
mod stored;
mod traits;
mod views;
mod wildcards;

pub fn lifecycles(text: &str) -> Result<gates::lifecycles::Lifecycles, gates::Unread> {
    gates::lifecycles::Lifecycles::of(vec![gates::declared::Member::of("fixture", vec![source(text)]).expect("the fixture parses")])
}

pub fn workspace(packages: &str) -> gates::workspace::Workspace {
    serde_json::from_str(&format!("{{\"workspace_root\": \"/w\", \"packages\": [{packages}]}}")).expect("the fixture is cargo metadata")
}

pub fn package(name: &str, role: &str, dependencies: &[(&str, Option<&str>)]) -> String {
    let held: Vec<String> = dependencies
        .iter()
        .map(|held| {
            format!(
                "{{\"name\": \"{}\", \"kind\": {}}}",
                held.0,
                held.1.map_or(String::from("null"), |kind| format!("\"{kind}\""))
            )
        })
        .collect();
    format!(
        "{{\"name\": \"{name}\", \"manifest_path\": \"/w/{name}/Cargo.toml\", \"targets\": [], \"dependencies\": [{}], \"metadata\": {{\"hex-arch\": {{\"role\": \"{role}\"}}}}}}",
        held.join(", ")
    )
}

pub fn source(text: &str) -> Source {
    Source::of(String::from("fixture.rs"), String::from(text)).expect("the fixture lexes")
}
