use std::path::Path;

use serde::Deserialize;

use crate::Unread;

const FILES: [&str; 4] = ["shell", "stores", "http", "gates"];

/// How a mutant must die: `test`, a compiler error code or `unused`; a gate that must fail and print a text; or tests that need a server.
#[derive(Debug, PartialEq, Eq, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub enum Expect {
    Kind(String),
    Gate { gate: Vec<String>, names: String },
    Server { server: Vec<String>, needs: String },
}

#[derive(Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mutant {
    pub name: String,
    pub path: String,
    pub anchor: String,
    pub replacement: String,
    #[serde(default)]
    pub example: bool,
    pub expect: Expect,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Planted {
    mutant: Vec<Mutant>,
}

impl Mutant {
    pub fn id(&self) -> &str {
        self.name.split(' ').next().unwrap_or("")
    }
}

pub fn parse(shown: &str, text: &str) -> Result<Vec<Mutant>, Unread> {
    toml::from_str::<Planted>(text)
        .map(|planted| planted.mutant)
        .map_err(|error| Unread(format!("{shown}: {error}")))
}

/// Every mutant under `planted/`, in the order the files and their entries are written.
pub fn read(root: &Path) -> Result<Vec<Mutant>, Unread> {
    let mut mutants: Vec<Mutant> = Vec::new();
    for file in FILES {
        let shown: String = format!("planted/{file}.toml");
        let text: String = std::fs::read_to_string(root.join(&shown)).map_err(|error| Unread(format!("{shown}: {error}")))?;
        mutants.append(&mut parse(&shown, &text)?);
    }
    Ok(mutants)
}
