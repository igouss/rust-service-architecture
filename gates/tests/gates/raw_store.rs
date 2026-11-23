use gates::raw_store::{cases, forbidden, tally};

use crate::{package, source, workspace};

fn pairs(held: &[(&str, &str)]) -> Vec<(String, String)> {
    held.iter().map(|pair| (String::from(pair.0), String::from(pair.1))).collect()
}

#[test]
fn an_adapter_that_depends_on_the_ports_alone_is_clean() {
    assert_eq!(
        forbidden(&workspace(
            &[
                package("store", "driven-adapter", &[("ports", None)]),
                package("ports", "port-and-adapter", &[])
            ]
            .join(",")
        )),
        Vec::new()
    );
}

#[test]
fn an_adapter_that_depends_on_a_serializer_is_named() {
    assert_eq!(
        forbidden(&workspace(&package("store", "driven-adapter", &[("ports", None), ("serde_json", None)]))),
        pairs(&[("store", "serde_json")])
    );
}

#[test]
fn an_adapter_that_depends_on_the_core_is_named_for_each_edge() {
    assert_eq!(
        forbidden(&workspace(
            &[
                package("domain", "domain", &[]),
                package("store", "driven-adapter", &[("domain", None), ("serde", Some("build"))])
            ]
            .join(",")
        )),
        pairs(&[("store", "domain"), ("store", "serde")])
    );
}

#[test]
fn a_dependency_of_the_tests_is_not_named() {
    assert_eq!(
        forbidden(&workspace(
            &[
                package("domain", "domain", &[]),
                package("store", "driven-adapter", &[("domain", Some("dev")), ("serde_json", Some("dev"))])
            ]
            .join(",")
        )),
        Vec::new()
    );
}

#[test]
fn a_crate_of_another_role_may_serialize() {
    assert_eq!(forbidden(&workspace(&package("http", "driving-adapter", &[("serde_json", None)]))), Vec::new());
}

#[test]
fn a_file_with_no_test_holds_no_case() {
    assert_eq!(cases(&source("#[cfg(test)]\nmod m { fn helper() {} }").tokens), 0);
}

#[test]
fn a_test_is_one_case() {
    assert_eq!(cases(&source("#[test]\nfn a() {}").tokens), 1);
}

#[test]
fn every_test_is_a_case_and_no_other_attribute_is() {
    assert_eq!(
        cases(&source("#[test]\nfn a() {}\nmod m { #[test] fn b() {} #[tokio::test] async fn c() {} #[test_case] fn d() {} }").tokens),
        2
    );
}

#[test]
fn a_run_that_reports_nothing_passed_nothing() {
    assert_eq!(tally("error: target `conformance` requires the features: `server`"), Ok((0, 0)));
}

#[test]
fn a_run_reports_what_passed_and_what_failed() {
    assert_eq!(
        tally("test result: FAILED. 54 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out"),
        Ok((54, 2))
    );
}

#[test]
fn the_reports_of_several_binaries_are_summed() {
    assert_eq!(
        tally("test result: ok. 50 passed; 0 failed; 0 ignored\n\ntest result: ok. 6 passed; 0 failed; 1 ignored"),
        Ok((56, 0))
    );
}
