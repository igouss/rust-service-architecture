#[test]
fn a_stream_never_written_loads_the_initial_state_at_version_zero() {
    assert_eq!(counter(&subject(), "c-1"), (Version::ZERO, Counter::Empty));
}

#[test]
fn an_applied_outcome_returns_its_recorded_facts() {
    assert_eq!(seen(&add(&subject(), "c-1", 5)), vec![added(1, "c-1", 1, 5)]);
}

#[test]
fn an_applied_outcome_is_stored_as_the_state_at_its_version() {
    let store: Subject = subject();
    add(&store, "c-1", 5);
    assert_eq!(counter(&store, "c-1"), (Version::from(1), Counter::Holding { total: 5 }));
}

#[test]
fn a_second_fact_of_a_stream_takes_the_next_version() {
    let store: Subject = subject();
    add(&store, "c-1", 5);
    assert_eq!(seen(&add(&store, "c-1", 2)), vec![added(2, "c-1", 2, 2)]);
    assert_eq!(counter(&store, "c-1"), (Version::from(2), Counter::Holding { total: 7 }));
}

#[test]
fn one_outcome_with_two_facts_takes_two_versions_and_two_positions() {
    let store: Subject = subject();
    let applied: Applied<Counter> = store
        .write(|unit| unit.apply(&id("c-1"), |state: Counter| ports::decide(AddTwice { by: 3 }, state)))
        .expect("applied");
    assert_eq!(seen(&applied.facts), vec![added(1, "c-1", 1, 3), added(2, "c-1", 2, 3)]);
    assert_eq!(counter(&store, "c-1"), (Version::from(2), Counter::Holding { total: 6 }));
}

#[test]
fn a_refused_outcome_leaves_the_state_and_the_log_unchanged() {
    let store: Subject = subject();
    add(&store, "c-1", 5);
    assert_eq!(add(&store, "c-1", 0).len(), 0);
    assert_eq!(counter(&store, "c-1"), (Version::from(1), Counter::Holding { total: 5 }));
    assert_eq!(counter_facts(&store, 0, 10), vec![added(1, "c-1", 1, 5)]);
}

#[test]
fn a_refusal_is_recorded_under_its_lifecycle_and_stream() {
    let store: Subject = subject();
    add(&store, "c-1", 0);
    add(&store, "c-2", 5);
    assert_eq!(
        store.read(|view: &mut View<'_>| view.refusals::<Counter>(&id("c-1"))),
        Ok(vec![CounterRefusal::NothingToAdd])
    );
    assert_eq!(store.read(|view: &mut View<'_>| view.refusals::<Counter>(&id("c-2"))), Ok(Vec::new()));
    assert_eq!(store.read(|view: &mut View<'_>| view.refusals::<Courier>(&id("c-1"))), Ok(Vec::new()));
}

#[test]
fn two_lifecycles_may_share_a_stream_id() {
    let store: Subject = subject();
    add(&store, "x", 5);
    send(&store, "x", 1);
    assert_eq!(counter(&store, "x"), (Version::from(1), Counter::Holding { total: 5 }));
    assert_eq!(
        store.read(|view| view.state::<Courier>(&id("x"))),
        Ok((Version::from(1), Courier::Sent { parcels: 1 }))
    );
}

#[test]
fn with_no_streams_no_state_is_listed() {
    assert_eq!(subject().read(|view| view.states::<Counter>()), Ok(Vec::new()));
}

#[test]
fn states_are_listed_in_stream_order_for_their_lifecycle_only() {
    let store: Subject = subject();
    add(&store, "c-2", 2);
    send(&store, "c-0", 1);
    add(&store, "c-1", 1);
    assert_eq!(
        store.read(|view| view.states::<Counter>()),
        Ok(vec![(id("c-1"), Counter::Holding { total: 1 }), (id("c-2"), Counter::Holding { total: 2 })])
    );
}

#[test]
fn with_no_facts_the_log_returns_none() {
    assert_eq!(counter_facts(&subject(), 0, 10), Vec::new());
}

#[test]
fn facts_come_back_in_commit_order_for_their_lifecycle_only() {
    let store: Subject = subject();
    add(&store, "c-1", 1);
    send(&store, "p-1", 1);
    add(&store, "c-2", 2);
    assert_eq!(counter_facts(&store, 0, 10), vec![added(1, "c-1", 1, 1), added(3, "c-2", 1, 2)]);
    assert_eq!(
        store
            .write(|unit| unit.facts::<Courier>(Position::ZERO, 10))
            .map(|facts: Vec<Recorded<Dispatched>>| facts.len()),
        Ok(1)
    );
}

#[test]
fn facts_resume_after_a_position_and_honour_the_limit() {
    let store: Subject = subject();
    add(&store, "c-1", 1);
    add(&store, "c-2", 2);
    add(&store, "c-3", 3);
    assert_eq!(counter_facts(&store, 1, 1), vec![added(2, "c-2", 1, 2)]);
}
