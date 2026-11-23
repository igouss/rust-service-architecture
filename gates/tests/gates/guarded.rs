use gates::guarded::{listed, tables};

const MANIFEST: &str =
    "[package]\nname = \"a\"\n\n[package.metadata.hex-arch]\nrole = \"domain\"\n\n[dependencies]\nserde = \"1\"\n\n[lints]\nworkspace = true\n";

#[test]
fn a_file_of_the_code_is_not_a_gate() {
    assert!(!listed("domain/src/task.rs"));
}

#[test]
fn the_recipes_and_the_lint_configuration_are_gates() {
    assert_eq!(
        [listed("justfile"), listed("clippy.toml"), listed("rustfmt.toml"), listed("domain/clippy.toml")],
        [true; 4]
    );
}

#[test]
fn a_script_at_the_top_is_a_gate_and_one_below_is_not() {
    assert_eq!([listed("check.py"), listed("sync.bb"), listed("bench/plot.py")], [true, true, false]);
}

#[test]
fn the_mutants_the_workflows_and_the_gates_crate_are_gates() {
    assert_eq!(
        [
            listed("planted/gates.toml"),
            listed(".github/workflows/ci.yml"),
            listed("gates/src/pins.rs"),
            listed("gates/Cargo.toml")
        ],
        [true; 4]
    );
}

#[test]
fn a_document_is_not_a_gate() {
    assert_eq!(
        [listed("README.md"), listed("docs/adr/0001-x.md"), listed(".github/dependabot.yml")],
        [false; 3]
    );
}

#[test]
fn a_manifest_that_is_not_there_holds_three_empty_tables() {
    assert_eq!(tables("").map(|held| held.len()), Ok(3));
}

#[test]
fn a_changed_dependency_leaves_the_tables_as_they_were() {
    assert_eq!(tables(&MANIFEST.replace("serde = \"1\"", "serde = \"2\"\ntokio = \"1\"")), tables(MANIFEST));
}

#[test]
fn a_changed_role_changes_the_tables() {
    assert_ne!(tables(&MANIFEST.replace("role = \"domain\"", "role = \"infra\"")), tables(MANIFEST));
}

#[test]
fn dropped_lints_change_the_tables() {
    assert_ne!(tables(&MANIFEST.replace("[lints]\nworkspace = true\n", "")), tables(MANIFEST));
}

#[test]
fn a_changed_workspace_lint_changes_the_tables() {
    assert_ne!(
        tables("[workspace.lints.clippy]\nunwrap_used = \"deny\"\n"),
        tables("[workspace.lints.clippy]\nunwrap_used = \"warn\"\n")
    );
}

#[test]
fn a_manifest_that_is_not_toml_is_unread() {
    assert!(tables("[package").is_err());
}
