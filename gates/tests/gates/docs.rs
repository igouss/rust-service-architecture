use gates::docs::{cited_mutants, cited_tests, normalized, open_ended, problems, rust_blocks, Held};

fn held() -> Held {
    Held {
        sources: vec![normalized(
            "pub struct Title(String);\n\nimpl Title {\n    pub fn new() -> Title {\n        todo()\n    }\n}\n",
        )],
        rust: String::from("fn a_title_of_two_hundred_characters_is_accepted() {}"),
        planted: vec![String::from("C1"), String::from("T2"), String::from("G30")],
    }
}

fn words(held: &[&str]) -> Vec<String> {
    held.iter().map(ToString::to_string).collect()
}

#[test]
fn normalizing_trims_each_line_and_drops_the_blank_ones() {
    assert_eq!(normalized("  a {\n\n      b\n   \n}\n"), "a {\nb\n}");
}

#[test]
fn a_block_may_stop_before_the_braces_that_close_its_item() {
    assert_eq!(
        [open_ended("a {\nb {\nc\n}\n}"), open_ended("a"), open_ended("}"), open_ended("")],
        [String::from("a {\nb {\nc"), String::from("a"), String::new(), String::new()]
    );
}

#[test]
fn a_document_with_no_rust_block_holds_none() {
    assert_eq!(rust_blocks("text\n```toml\na = 1\n```\n"), Ok(Vec::new()));
}

#[test]
fn each_rust_block_is_read_without_its_fence() {
    assert_eq!(
        rust_blocks("```rust\nfn a() {}\n```\ntext\n```rust\nfn b() {}\nfn c() {}\n```\n"),
        Ok(words(&["fn a() {}\n", "fn b() {}\nfn c() {}\n"]))
    );
}

#[test]
fn a_name_of_five_words_in_backticks_is_a_cited_test_and_a_shorter_one_is_not() {
    assert_eq!(
        cited_tests("`a_b_c_d` and `a_title_is_at_most_two`, again `a_title_is_at_most_two`, then `one_two_three_four_five` but not Plain_five_words_of_text"),
        Ok(words(&["a_title_is_at_most_two", "one_two_three_four_five"]))
    );
}

#[test]
fn text_that_cites_no_mutant_names_none() {
    assert_eq!(
        cited_mutants("the mutation job plants T-shaped faults; C1 alone is not a citation"),
        Ok(Vec::new())
    );
}

#[test]
fn a_list_names_each_mutant_and_a_range_names_its_ends() {
    assert_eq!(
        cited_mutants("(T3; mutant G27), mutants G9, G16 and G18, then mutants G1 to G30 and mutant G9 again"),
        Ok(words(&["G27", "G9", "G16", "G18", "G1", "G30"]))
    );
}

#[test]
fn a_document_whose_block_and_citations_exist_has_no_problem() {
    assert_eq!(
        problems(
            "core.req.md",
            "```rust\nimpl Title {\n    pub fn new() -> Title {\n        todo()\n```\n`a_title_of_two_hundred_characters_is_accepted` (mutants C1 and T2)",
            &held()
        ),
        Ok((Vec::new(), Vec::new()))
    );
}

#[test]
fn a_block_the_sources_do_not_hold_is_named_by_its_number_and_first_line() {
    assert_eq!(
        problems(
            "core.req.md",
            "```rust\npub struct Title(String);\n```\n```rust\npub struct Title(Vec<u8>);\nmore\n```\n",
            &held()
        ),
        Ok((words(&["core.req.md rust block 2: pub struct Title(Vec<u8>);"]), Vec::new()))
    );
}

#[test]
fn a_test_and_a_mutant_that_do_not_exist_are_each_named() {
    assert_eq!(
        problems("core.req.md", "`a_title_of_two_hundred_and_one_is_refused` (mutants C1, T9 and G31)", &held()),
        Ok((
            Vec::new(),
            words(&[
                "core.req.md names a test that does not exist: a_title_of_two_hundred_and_one_is_refused",
                "core.req.md names a mutant that planted/ does not hold: T9",
                "core.req.md names a mutant that planted/ does not hold: G31",
            ])
        ))
    );
}
