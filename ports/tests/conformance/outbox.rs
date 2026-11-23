#[test]
fn with_no_requests_nothing_is_claimed() {
    assert_eq!(parcels(&subject(), 10), Vec::new());
}

#[test]
fn a_request_is_claimed_with_its_stream() {
    let store: Subject = subject();
    send(&store, "p-1", 7);
    assert_eq!(parcels(&store, 10), vec![(id("p-1"), Parcel { number: 7 })]);
}

#[test]
fn many_requests_are_claimed_in_the_order_they_were_issued_with_distinct_ids() {
    let store: Subject = subject();
    send(&store, "p-1", 1);
    send(&store, "p-2", 2);
    let claimed: Vec<Claimed<Parcel>> = store.claim::<Send>(10).expect("claimed");
    assert_eq!((claimed[0].request.number, claimed[1].request.number), (1, 2));
    assert_ne!(claimed[0].id, claimed[1].id);
}

#[test]
fn a_claim_honours_its_limit() {
    let store: Subject = subject();
    send(&store, "p-1", 1);
    send(&store, "p-2", 2);
    assert_eq!(parcels(&store, 1), vec![(id("p-1"), Parcel { number: 1 })]);
}

#[test]
fn a_claim_returns_only_its_lifecycles_requests() {
    let store: Subject = subject();
    send(&store, "p-1", 1);
    let _carried: Applied<Porter> = store
        .write(|unit| unit.apply(&id("b-1"), |state: Porter| ports::react(Carry { tag: 9 }, state)))
        .expect("applied");
    assert_eq!(parcels(&store, 10), vec![(id("p-1"), Parcel { number: 1 })]);
    assert_eq!(store.claim::<Carry>(10).map(|claimed: Vec<Claimed<Bag>>| claimed.len()), Ok(1));
}

#[test]
fn a_done_request_is_not_claimed_again() {
    let store: Subject = subject();
    send(&store, "p-1", 1);
    send(&store, "p-2", 2);
    let claimed: Vec<Claimed<Parcel>> = store.claim::<Send>(10).expect("claimed");
    assert_eq!(receive(&store, claimed[0].id, 5), Some(1));
    assert_eq!(parcels(&store, 10), vec![(id("p-2"), Parcel { number: 2 })]);
}

#[test]
fn a_finished_request_applies_its_reaction_to_the_stream_that_issued_it() {
    let store: Subject = subject();
    send(&store, "p-1", 1);
    send(&store, "p-2", 2);
    let claimed: Vec<Claimed<Parcel>> = store.claim::<Send>(10).expect("claimed");
    assert_eq!(receive(&store, claimed[1].id, 5), Some(1));
    assert_eq!(courier(&store, "p-1"), (Version::from(1), Courier::Sent { parcels: 1 }));
    assert_eq!(courier(&store, "p-2"), (Version::from(2), Courier::Sent { parcels: 6 }));
}

#[test]
fn a_request_finished_twice_applies_its_reaction_once() {
    let store: Subject = subject();
    send(&store, "p-1", 1);
    let claimed: Vec<Claimed<Parcel>> = store.claim::<Send>(10).expect("claimed");
    assert_eq!(receive(&store, claimed[0].id, 5), Some(1));
    assert_eq!(receive(&store, claimed[0].id, 5), None);
    assert_eq!(courier(&store, "p-1"), (Version::from(2), Courier::Sent { parcels: 6 }));
}

#[test]
fn a_request_finished_twice_in_one_unit_applies_its_reaction_once() {
    let store: Subject = subject();
    send(&store, "p-1", 1);
    let claimed: Vec<Claimed<Parcel>> = store.claim::<Send>(10).expect("claimed");
    let twice: (Option<usize>, Option<usize>) = store
        .write(|unit| {
            let first: Finished<Courier> = unit.finish(claimed[0].id, |state: Courier| ports::react(Receive { count: 5 }, state))?;
            let second: Finished<Courier> = unit.finish(claimed[0].id, |state: Courier| ports::react(Receive { count: 5 }, state))?;
            Ok((facts_of(first), facts_of(second)))
        })
        .expect("finished");
    assert_eq!(twice, (Some(1), None));
    assert_eq!(courier(&store, "p-1"), (Version::from(2), Courier::Sent { parcels: 6 }));
}

#[test]
fn a_request_that_was_never_issued_finishes_nothing() {
    let store: Subject = subject();
    assert_eq!(receive(&store, RequestId::from(1), 5), None);
    assert_eq!(courier(&store, "p-1"), (Version::ZERO, Courier::Idle));
}

#[test]
fn a_request_is_finished_only_by_the_lifecycle_that_issued_it() {
    let store: Subject = subject();
    send(&store, "p-1", 1);
    let claimed: Vec<Claimed<Parcel>> = store.claim::<Send>(10).expect("claimed");
    let other: Finished<Porter> = store
        .write(|unit| unit.finish(claimed[0].id, |state: Porter| ports::react(Carry { tag: 9 }, state)))
        .expect("finished");
    assert_eq!(facts_of(other), None);
    assert_eq!(store.claim::<Carry>(10).map(|claimed: Vec<Claimed<Bag>>| claimed.len()), Ok(0));
    assert_eq!(parcels(&store, 10), vec![(id("p-1"), Parcel { number: 1 })]);
}

#[test]
fn a_unit_that_fails_after_finishing_a_request_leaves_it_claimable() {
    let store: Subject = subject();
    send(&store, "p-1", 1);
    let claimed: Vec<Claimed<Parcel>> = store.claim::<Send>(10).expect("claimed");
    let failed: Result<(), StoreError> = store.write(|unit| {
        let _finished: Finished<Courier> = unit.finish(claimed[0].id, |state: Courier| ports::react(Receive { count: 5 }, state))?;
        Err(StoreError::Unavailable { cause: "injected".to_string() })
    });
    assert_eq!(failed, Err(StoreError::Unavailable { cause: "injected".to_string() }));
    assert_eq!(courier(&store, "p-1"), (Version::from(1), Courier::Sent { parcels: 1 }));
    assert_eq!(parcels(&store, 10), vec![(id("p-1"), Parcel { number: 1 })]);
}

#[test]
fn a_failed_request_is_claimed_after_those_that_failed_less() {
    let store: Subject = subject();
    send(&store, "p-1", 1);
    send(&store, "p-2", 2);
    let claimed: Vec<Claimed<Parcel>> = store.claim::<Send>(10).expect("claimed");
    store.failed(claimed[0].id).expect("failed");
    assert_eq!(parcels(&store, 10), vec![(id("p-2"), Parcel { number: 2 }), (id("p-1"), Parcel { number: 1 })]);
}

#[test]
fn a_step_that_requests_without_a_fact_keeps_the_version_and_stores_the_request() {
    let store: Subject = subject();
    let applied: Applied<Courier> = store
        .write(|unit| unit.apply(&id("p-1"), |state: Courier| ports::react(Remind { number: 3 }, state)))
        .expect("applied");
    assert_eq!(applied.facts.len(), 0);
    assert_eq!(store.read(|view| view.state::<Courier>(&id("p-1"))), Ok((Version::ZERO, Courier::Idle)));
    assert_eq!(parcels(&store, 10), vec![(id("p-1"), Parcel { number: 3 })]);
}
