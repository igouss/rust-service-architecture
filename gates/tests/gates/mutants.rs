use gates::mutants::{judged, listed};
use gates::planted::{parse, Expect, Mutant};

const TESTS: &str =
    "---- store::b_case stdout ----\npanicked\n---- a_case stdout ----\npanicked\n---- a_case stdout ----\ntest result: FAILED. 3 passed; 2 failed;";

fn kind(kind: &str) -> Expect {
    Expect::Kind(String::from(kind))
}

fn gate() -> Expect {
    Expect::Gate {
        gate: vec![String::from("just"), String::from("names")],
        names: String::from("arch-1bmf"),
    }
}

fn server() -> Expect {
    Expect::Server {
        server: vec![String::from("cargo"), String::from("test")],
        needs: String::from("STORE_URL"),
    }
}

fn verdict(expect: &Expect, code: i32, output: &str) -> (bool, String) {
    judged(expect, true, code, output).expect("the patterns compile")
}

#[test]
fn a_run_that_passes_leaves_the_mutant_alive() {
    assert_eq!(
        [verdict(&kind("test"), 0, ""), verdict(&kind("E0308"), 0, ""), verdict(&kind("unused"), 0, "")],
        [(); 3].map(|()| (false, String::from("SURVIVED")))
    );
}

#[test]
fn failing_tests_kill_and_are_named_once_each_in_order() {
    assert_eq!(verdict(&kind("test"), 101, TESTS), (true, String::from("killed by a_case, store::b_case")));
}

#[test]
fn a_test_mutant_that_does_not_compile_is_not_killed() {
    assert_eq!(
        verdict(
            &kind("test"),
            101,
            "error[E0308]: mismatched\nerror[E0308]: again\nerror[E0061]: x\nerror: could not compile `app`"
        ),
        (false, String::from("DID NOT COMPILE: error[E0061] error[E0308]"))
    );
}

#[test]
fn a_red_run_with_no_failed_test_does_not_kill() {
    assert_eq!(verdict(&kind("test"), 101, "error: something else"), (false, String::from("killed by ")));
}

#[test]
fn the_expected_error_code_kills() {
    assert_eq!(verdict(&kind("E0308"), 101, "error[E0308]: mismatched types"), (true, String::from("E0308")));
}

#[test]
fn another_error_code_does_not_kill() {
    assert_eq!(
        verdict(&kind("E0308"), 101, "error[E0599]: no method\nerror[E0061]: arguments"),
        (false, String::from("E0061 E0599"))
    );
}

#[test]
fn an_unused_outcome_kills_a_mutant_that_expects_it() {
    assert_eq!(
        verdict(&kind("unused"), 101, "error: unused `Applied` that must be used"),
        (true, String::from("unused_must_use"))
    );
}

#[test]
fn an_error_code_does_not_kill_a_mutant_that_expects_an_unused_outcome() {
    assert_eq!(verdict(&kind("unused"), 101, "error[E0308]: mismatched types"), (false, String::from("E0308")));
}

#[test]
fn a_gate_kills_when_it_fails_and_prints_the_text() {
    assert_eq!(
        verdict(&gate(), 1, "arch-1bmf: two lifecycles"),
        (true, String::from("exit 1, names arch-1bmf"))
    );
}

#[test]
fn a_gate_that_fails_for_another_reason_does_not_kill() {
    assert_eq!(verdict(&gate(), 2, "names: unread"), (false, String::from("exit 2, DOES NOT NAME arch-1bmf")));
}

#[test]
fn a_gate_that_prints_the_text_and_passes_does_not_kill() {
    assert_eq!(verdict(&gate(), 0, "arch-1bmf"), (false, String::from("exit 0, names arch-1bmf")));
}

#[test]
fn a_server_mutant_with_no_server_named_is_not_run() {
    assert_eq!(
        judged(&server(), false, 0, ""),
        Ok((false, String::from("NEEDS STORE_URL: this mutant is killed by tests that run against a server")))
    );
}

#[test]
fn a_server_mutant_is_killed_by_failing_tests_when_others_passed() {
    assert_eq!(verdict(&server(), 101, TESTS), (true, String::from("killed by a_case, store::b_case")));
}

#[test]
fn a_server_mutant_with_no_passing_test_proves_nothing() {
    assert_eq!(
        verdict(&server(), 101, "---- a_case stdout ----\ntest result: FAILED. 0 passed; 9 failed;"),
        (false, String::from("NO TEST PASSED: the server is not reachable, so a failure proves nothing"))
    );
}

#[test]
fn a_server_mutant_whose_tests_all_pass_survived() {
    assert_eq!(verdict(&server(), 0, "test result: ok. 9 passed; 0 failed;"), (false, String::from("SURVIVED")));
}

#[test]
fn a_file_with_no_mutant_is_unread() {
    assert!(parse("planted/x.toml", "").is_err());
}

#[test]
fn a_mutant_is_read_with_each_way_of_dying() {
    assert_eq!(
        parse("planted/x.toml", "[[mutant]]\nname = \"T1 one\"\npath = \"a.rs\"\nanchor = 'x'\nreplacement = \"y\\nz\"\nexpect = \"test\"\n\n[[mutant]]\nname = \"G1 two\"\npath = \"b.rs\"\nanchor = \"x\"\nreplacement = \"\"\nexample = true\nexpect = { gate = [\"just\", \"names\"], names = \"arch-1bmf\" }\n\n[[mutant]]\nname = \"T2 three\"\npath = \"c.rs\"\nanchor = \"x\"\nreplacement = \"y\"\nexpect = { server = [\"cargo\", \"test\"], needs = \"STORE_URL\" }\n"),
        Ok(vec![
            Mutant { name: String::from("T1 one"), path: String::from("a.rs"), anchor: String::from("x"), replacement: String::from("y\nz"), example: false, expect: kind("test") },
            Mutant { name: String::from("G1 two"), path: String::from("b.rs"), anchor: String::from("x"), replacement: String::new(), example: true, expect: gate() },
            Mutant { name: String::from("T2 three"), path: String::from("c.rs"), anchor: String::from("x"), replacement: String::from("y"), example: false, expect: server() },
        ])
    );
}

#[test]
fn a_mutant_with_a_field_nobody_reads_is_unread() {
    assert!(parse(
        "planted/x.toml",
        "[[mutant]]\nname = \"T1 one\"\npath = \"a.rs\"\nanchor = \"x\"\nreplacement = \"y\"\nexpect = \"test\"\nexpects = \"E0308\"\n"
    )
    .is_err());
}

#[test]
fn the_list_marks_each_mutant_and_names_its_file() {
    assert_eq!(
        listed(&parse("planted/x.toml", "[[mutant]]\nname = \"T1 one\"\npath = \"a.rs\"\nanchor = \"x\"\nreplacement = \"y\"\nexpect = \"test\"\n\n[[mutant]]\nname = \"G1 two\"\npath = \"domain/src/fulfilment.rs\"\nanchor = \"x\"\nreplacement = \"y\"\nexample = true\nexpect = \"test\"\n").expect("the fixture is read")),
        vec![String::from("machinery a.rs                         T1 one"), String::from("example   domain/src/fulfilment.rs     G1 two")]
    );
}
