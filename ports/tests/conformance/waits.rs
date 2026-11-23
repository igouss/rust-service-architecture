fn due(store: &Subject, now: u64) -> Vec<u64> {
    store
        .claim::<Send>(10, Moment::ZERO.after(now), Moment::ZERO)
        .expect("claimed")
        .into_iter()
        .map(|claimed: Claimed<Parcel>| claimed.request.number)
        .collect()
}

fn put_off(store: &Subject, request: u32, until: u64) {
    store.failed(RequestId::from(request), Moment::ZERO.after(until)).expect("failed");
}

#[test]
fn a_request_put_off_is_not_claimed_before_its_moment() {
    let store: Subject = subject();
    send(&store, "p-1", 1);
    put_off(&store, 1, 100);
    assert_eq!(due(&store, 99), Vec::<u64>::new());
}

#[test]
fn a_request_put_off_is_claimed_at_its_moment() {
    let store: Subject = subject();
    send(&store, "p-1", 1);
    put_off(&store, 1, 100);
    assert_eq!(due(&store, 100), vec![1]);
}

#[test]
fn a_request_put_off_does_not_hold_back_one_that_is_not() {
    let store: Subject = subject();
    send(&store, "p-1", 1);
    send(&store, "p-2", 2);
    put_off(&store, 1, 100);
    assert_eq!(due(&store, 50), vec![2]);
}

#[test]
fn requests_put_off_to_different_moments_are_each_claimed_from_their_own() {
    let store: Subject = subject();
    send(&store, "p-1", 1);
    send(&store, "p-2", 2);
    put_off(&store, 1, 100);
    put_off(&store, 2, 200);
    assert_eq!((due(&store, 150), due(&store, 200)), (vec![1], vec![1, 2]));
}

#[test]
fn a_request_put_off_again_waits_for_the_later_moment() {
    let store: Subject = subject();
    send(&store, "p-1", 1);
    put_off(&store, 1, 100);
    put_off(&store, 1, 300);
    assert_eq!((due(&store, 200), due(&store, 300)), (Vec::<u64>::new(), vec![1]));
}

#[test]
fn putting_off_a_request_that_was_never_issued_changes_nothing() {
    let store: Subject = subject();
    send(&store, "p-1", 1);
    assert_eq!(store.failed(RequestId::from(9), Moment::ZERO.after(100)), Ok(()));
    assert_eq!(due(&store, 0), vec![1]);
}

fn leased(store: &Subject, limit: usize, now: u64, until: u64) -> Vec<u64> {
    store
        .claim::<Send>(limit, Moment::ZERO.after(now), Moment::ZERO.after(until))
        .expect("claimed")
        .into_iter()
        .map(|claimed: Claimed<Parcel>| claimed.request.number)
        .collect()
}

#[test]
fn a_request_claimed_under_a_lease_is_not_claimed_again_before_the_lease_ends() {
    let store: Subject = subject();
    send(&store, "p-1", 1);
    assert_eq!((leased(&store, 10, 0, 100), leased(&store, 10, 99, 199)), (vec![1], Vec::<u64>::new()));
}

#[test]
fn a_request_claimed_under_a_lease_is_claimed_again_when_the_lease_ends() {
    let store: Subject = subject();
    send(&store, "p-1", 1);
    assert_eq!((leased(&store, 10, 0, 100), leased(&store, 10, 100, 200)), (vec![1], vec![1]));
}

#[test]
fn a_lease_holds_only_the_requests_it_claimed() {
    let store: Subject = subject();
    send(&store, "p-1", 1);
    send(&store, "p-2", 2);
    assert_eq!((leased(&store, 1, 0, 100), leased(&store, 10, 0, 100)), (vec![1], vec![2]));
}

#[test]
fn many_requests_claimed_under_one_lease_are_all_held() {
    let store: Subject = subject();
    send(&store, "p-1", 1);
    send(&store, "p-2", 2);
    assert_eq!((leased(&store, 10, 0, 100), leased(&store, 10, 50, 150)), (vec![1, 2], Vec::<u64>::new()));
}

#[test]
fn a_leased_request_that_fails_is_claimed_from_the_moment_it_was_put_off_to() {
    let store: Subject = subject();
    send(&store, "p-1", 1);
    assert_eq!(leased(&store, 10, 0, 100), vec![1]);
    put_off(&store, 1, 50);
    assert_eq!((due(&store, 49), due(&store, 50)), (Vec::<u64>::new(), vec![1]));
}

#[test]
fn a_claim_with_no_lease_leaves_a_request_put_off_where_it_was() {
    let store: Subject = subject();
    send(&store, "p-1", 1);
    put_off(&store, 1, 100);
    assert_eq!((due(&store, 100), due(&store, 99)), (vec![1], Vec::<u64>::new()));
}
