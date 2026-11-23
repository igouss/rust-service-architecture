use gates::pins::unpinned;
use gates::Unread;

const REV: &str = "c8a0000ad5f881378f99df86c60e461acb545c14";

fn workflow(lines: &[&str]) -> Vec<(String, String)> {
    vec![(String::from("ci.yml"), lines.join("\n"))]
}

#[test]
fn a_workflow_that_installs_nothing_is_clean() {
    assert_eq!(unpinned(&workflow(&["- run: just check"])), Ok(Vec::new()));
}

#[test]
fn an_install_from_the_registry_is_clean() {
    assert_eq!(unpinned(&workflow(&["- run: cargo install ripgrep"])), Ok(Vec::new()));
}

#[test]
fn an_install_from_git_at_a_full_commit_is_clean() {
    assert_eq!(
        unpinned(&workflow(&[&format!("- run: cargo install --locked --git https://x/y --rev {REV}")])),
        Ok(Vec::new())
    );
}

#[test]
fn an_install_from_git_with_no_rev_is_named_with_its_line() {
    assert_eq!(
        unpinned(&workflow(&["- run: just check", "- run: cargo install --git https://x/y"])),
        Ok(vec![String::from(
            "arch-mov6: ci.yml:2 cargo install --git https://x/y names no --rev of 40 hex digits"
        )])
    );
}

#[test]
fn two_unpinned_installs_are_both_named() {
    assert_eq!(
        unpinned(&workflow(&[
            "- run: cargo install --git https://x/a",
            &format!("- run: cargo install --git https://x/b --rev {REV}"),
            "- run: cargo install --git=https://x/c"
        ])),
        Ok(vec![
            String::from("arch-mov6: ci.yml:1 cargo install --git https://x/a names no --rev of 40 hex digits"),
            String::from("arch-mov6: ci.yml:3 cargo install --git https://x/c names no --rev of 40 hex digits"),
        ])
    );
}

#[test]
fn a_short_rev_is_not_a_pin() {
    assert_eq!(
        unpinned(&workflow(&["- run: cargo install --git https://x/y --rev c8a0000"])),
        Ok(vec![String::from(
            "arch-mov6: ci.yml:1 cargo install --git https://x/y names no --rev of 40 hex digits"
        )])
    );
}

#[test]
fn forty_one_digits_are_not_a_pin() {
    assert_eq!(
        unpinned(&workflow(&[&format!("- run: cargo install --git https://x/y --rev {REV}a")])),
        Ok(vec![String::from(
            "arch-mov6: ci.yml:1 cargo install --git https://x/y names no --rev of 40 hex digits"
        )])
    );
}

#[test]
fn two_installs_on_one_line_are_unread() {
    assert_eq!(
        unpinned(&workflow(&["- run: cargo install a && cargo install b"])),
        Err(Unread(String::from(
            "ci.yml:1: 2 `cargo install` on one line; one `--rev` cannot be read as pinning both"
        )))
    );
}

#[test]
fn an_install_that_goes_on_past_its_line_is_unread() {
    assert_eq!(
        unpinned(&workflow(&["- run: cargo install --git https://x/y \\", &format!("    --rev {REV}")])),
        Err(Unread(String::from(
            "ci.yml:1: `cargo install --git https://x/y \\` goes on past the line; write the install on one line"
        )))
    );
}

#[test]
fn a_git_source_off_the_line_of_an_install_is_unread() {
    assert_eq!(
        unpinned(&workflow(&["- run: cargo install", "    --git https://x/y"])),
        Err(Unread(String::from(
            "1 `--git` in the workflows, 0 on the line of a `cargo install`; write each install on one line"
        )))
    );
}
