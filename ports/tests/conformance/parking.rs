#[test]
fn a_request_that_never_failed_is_claimed_with_no_attempts() {
    let store: Subject = subject();
    send(&store, "p-1", 1);
    assert_eq!(attempts(&store), vec![0]);
}

#[test]
fn a_request_that_failed_once_is_claimed_with_one_attempt() {
    let store: Subject = subject();
    send(&store, "p-1", 1);
    store.failed(RequestId::from(1), Moment::ZERO).expect("failed");
    assert_eq!(attempts(&store), vec![1]);
}

#[test]
fn requests_that_failed_are_claimed_each_with_its_own_attempts() {
    let store: Subject = subject();
    send(&store, "p-1", 1);
    send(&store, "p-2", 2);
    store.failed(RequestId::from(1), Moment::ZERO).expect("failed");
    store.failed(RequestId::from(1), Moment::ZERO).expect("failed");
    assert_eq!(attempts(&store), vec![0, 2]);
}

#[test]
fn a_parked_request_is_not_claimed_again() {
    let store: Subject = subject();
    send(&store, "p-1", 1);
    send(&store, "p-2", 2);
    let claimed: Vec<Claimed<Parcel>> = store.claim::<Send>(10, Moment::ZERO).expect("claimed");
    assert_eq!(shelve(&store, claimed[0].id, 5), Some(1));
    assert_eq!(parcels(&store, 10), vec![(id("p-2"), Parcel { number: 2 })]);
}

#[test]
fn a_parked_request_applies_its_reaction_to_the_stream_that_issued_it() {
    let store: Subject = subject();
    send(&store, "p-1", 1);
    send(&store, "p-2", 2);
    let claimed: Vec<Claimed<Parcel>> = store.claim::<Send>(10, Moment::ZERO).expect("claimed");
    assert_eq!(shelve(&store, claimed[1].id, 5), Some(1));
    assert_eq!(courier(&store, "p-1"), (Version::from(1), Courier::Sent { parcels: 1 }));
    assert_eq!(courier(&store, "p-2"), (Version::from(2), Courier::Sent { parcels: 6 }));
}

#[test]
fn a_request_parked_twice_applies_its_reaction_once() {
    let store: Subject = subject();
    send(&store, "p-1", 1);
    let claimed: Vec<Claimed<Parcel>> = store.claim::<Send>(10, Moment::ZERO).expect("claimed");
    assert_eq!(shelve(&store, claimed[0].id, 5), Some(1));
    assert_eq!(shelve(&store, claimed[0].id, 5), None);
    assert_eq!(courier(&store, "p-1"), (Version::from(2), Courier::Sent { parcels: 6 }));
}

#[test]
fn a_request_finished_and_then_parked_applies_only_the_first_reaction() {
    let store: Subject = subject();
    send(&store, "p-1", 1);
    let claimed: Vec<Claimed<Parcel>> = store.claim::<Send>(10, Moment::ZERO).expect("claimed");
    assert_eq!(receive(&store, claimed[0].id, 5), Some(1));
    assert_eq!(shelve(&store, claimed[0].id, 7), None);
    assert_eq!(courier(&store, "p-1"), (Version::from(2), Courier::Sent { parcels: 6 }));
}

#[test]
fn a_request_parked_and_then_finished_applies_only_the_first_reaction() {
    let store: Subject = subject();
    send(&store, "p-1", 1);
    let claimed: Vec<Claimed<Parcel>> = store.claim::<Send>(10, Moment::ZERO).expect("claimed");
    assert_eq!(shelve(&store, claimed[0].id, 7), Some(1));
    assert_eq!(receive(&store, claimed[0].id, 5), None);
    assert_eq!(courier(&store, "p-1"), (Version::from(2), Courier::Sent { parcels: 8 }));
}

#[test]
fn a_request_finished_and_parked_in_one_unit_applies_only_the_first_reaction() {
    let store: Subject = subject();
    send(&store, "p-1", 1);
    let claimed: Vec<Claimed<Parcel>> = store.claim::<Send>(10, Moment::ZERO).expect("claimed");
    let both: (Option<usize>, Option<usize>) = store
        .write(|unit| {
            let first: Finished<Courier> = unit.finish(claimed[0].id, |state: Courier| ports::react(Receive { count: 5 }, state))?;
            let second: Finished<Courier> = unit.park(claimed[0].id, "refused", |state: Courier| ports::react(Receive { count: 7 }, state))?;
            Ok((facts_of(first), facts_of(second)))
        })
        .expect("closed");
    assert_eq!(both, (Some(1), None));
    assert_eq!(courier(&store, "p-1"), (Version::from(2), Courier::Sent { parcels: 6 }));
}

#[test]
fn a_request_is_parked_only_by_the_lifecycle_that_issued_it() {
    let store: Subject = subject();
    send(&store, "p-1", 1);
    let claimed: Vec<Claimed<Parcel>> = store.claim::<Send>(10, Moment::ZERO).expect("claimed");
    let other: Finished<Porter> = store
        .write(|unit| unit.park(claimed[0].id, "refused", |state: Porter| ports::react(Carry { tag: 9 }, state)))
        .expect("parked");
    assert_eq!(facts_of(other), None);
    assert_eq!(parcels(&store, 10), vec![(id("p-1"), Parcel { number: 1 })]);
}

#[test]
fn a_unit_that_fails_after_parking_a_request_leaves_it_claimable() {
    let store: Subject = subject();
    send(&store, "p-1", 1);
    let claimed: Vec<Claimed<Parcel>> = store.claim::<Send>(10, Moment::ZERO).expect("claimed");
    let failed: Result<(), StoreError> = store.write(|unit| {
        let _parked: Finished<Courier> = unit.park(claimed[0].id, "refused", |state: Courier| ports::react(Receive { count: 5 }, state))?;
        Err(StoreError::Unavailable { cause: "injected".to_string() })
    });
    assert_eq!(failed, Err(StoreError::Unavailable { cause: "injected".to_string() }));
    assert_eq!(courier(&store, "p-1"), (Version::from(1), Courier::Sent { parcels: 1 }));
    assert_eq!(attempts(&store), vec![0]);
}

#[test]
fn a_stranded_request_is_not_claimed_again_and_applies_nothing() {
    let store: Subject = subject();
    send(&store, "p-1", 1);
    send(&store, "p-2", 2);
    let claimed: Vec<Claimed<Parcel>> = store.claim::<Send>(10, Moment::ZERO).expect("claimed");
    assert_eq!(store.write(|unit| unit.strand::<Courier>(claimed[0].id, "cannot be told")), Ok(true));
    assert_eq!(parcels(&store, 10), vec![(id("p-2"), Parcel { number: 2 })]);
    assert_eq!(courier(&store, "p-1"), (Version::from(1), Courier::Sent { parcels: 1 }));
}

#[test]
fn a_request_stranded_twice_is_parked_once() {
    let store: Subject = subject();
    send(&store, "p-1", 1);
    let claimed: Vec<Claimed<Parcel>> = store.claim::<Send>(10, Moment::ZERO).expect("claimed");
    assert_eq!(store.write(|unit| unit.strand::<Courier>(claimed[0].id, "cannot be told")), Ok(true));
    assert_eq!(store.write(|unit| unit.strand::<Courier>(claimed[0].id, "cannot be told")), Ok(false));
}

#[test]
fn a_request_is_stranded_only_by_the_lifecycle_that_issued_it() {
    let store: Subject = subject();
    send(&store, "p-1", 1);
    let claimed: Vec<Claimed<Parcel>> = store.claim::<Send>(10, Moment::ZERO).expect("claimed");
    assert_eq!(store.write(|unit| unit.strand::<Porter>(claimed[0].id, "cannot be told")), Ok(false));
    assert_eq!(parcels(&store, 10), vec![(id("p-1"), Parcel { number: 1 })]);
}
