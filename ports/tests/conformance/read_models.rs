#[test]
fn a_row_never_put_reads_as_none() {
    assert_eq!(subject().read(|view| view.row::<Total>("all")), Ok(None));
    assert_eq!(subject().read(|view| view.rows::<Total>()), Ok(Vec::new()));
}

#[test]
fn a_put_row_is_listed_under_its_key() {
    let store: Subject = subject();
    let fact: Recorded<Counted> = first_fact(&store, "c-1", 5);
    store.write(|unit| unit.put(ports::project::<Total>(None, &fact))).expect("put");
    assert_eq!(store.read(|view| view.row::<Total>("all")), Ok(Some(5)));
    assert_eq!(store.read(|view| view.rows::<Total>()), Ok(vec![("all".to_string(), 5)]));
}

#[test]
fn a_put_row_replaces_the_row_under_the_same_key() {
    let store: Subject = subject();
    let first: Recorded<Counted> = first_fact(&store, "c-1", 5);
    let second: Recorded<Counted> = first_fact(&store, "c-1", 2);
    store.write(|unit| unit.put(ports::project::<Total>(None, &first))).expect("first put");
    store.write(|unit| unit.put(ports::project::<Total>(Some(5), &second))).expect("second put");
    assert_eq!(store.read(|view| view.rows::<Total>()), Ok(vec![("all".to_string(), 7)]));
}

#[test]
fn many_rows_are_listed_in_key_order() {
    let store: Subject = subject();
    let later: Recorded<Counted> = first_fact(&store, "c-2", 2);
    let earlier: Recorded<Counted> = first_fact(&store, "c-1", 1);
    store.write(|unit| unit.put(ports::project::<PerStream>(None, &later))).expect("first put");
    store.write(|unit| unit.put(ports::project::<PerStream>(None, &earlier))).expect("second put");
    assert_eq!(
        store.read(|view| view.rows::<PerStream>()),
        Ok(vec![("c-1".to_string(), 1), ("c-2".to_string(), 2)])
    );
}

#[test]
fn a_deleted_row_is_gone() {
    let store: Subject = subject();
    let fact: Recorded<Counted> = first_fact(&store, "c-1", 5);
    store.write(|unit| unit.put(ports::project::<PerStream>(None, &fact))).expect("put");
    let reset: Applied<Counter> = store
        .write(|unit| unit.apply(&id("c-1"), |state: Counter| ports::decide(Reset, state)))
        .expect("applied");
    let gone: Recorded<Counted> = reset.facts.into_iter().next().expect("one fact");
    store.write(|unit| unit.put(ports::project::<PerStream>(Some(5), &gone))).expect("deleted");
    assert_eq!(store.read(|view| view.rows::<PerStream>()), Ok(Vec::new()));
}

#[test]
fn a_change_at_or_below_the_read_models_position_is_refused() {
    let store: Subject = subject();
    let fact: Recorded<Counted> = first_fact(&store, "c-1", 5);
    store.write(|unit| unit.put(ports::project::<Total>(None, &fact))).expect("first put");
    let second: Result<(), StoreError> = store.write(|unit| unit.put(ports::project::<Total>(None, &fact)));
    assert_eq!(
        second,
        Err(StoreError::OutOfOrder {
            read_model: "suite_total_v1",
            at: Position::from(1),
            offered: Position::from(1)
        })
    );
}

#[test]
fn each_read_model_keeps_its_own_position() {
    let store: Subject = subject();
    let fact: Recorded<Counted> = first_fact(&store, "c-1", 5);
    store.write(|unit| unit.put(ports::project::<Total>(None, &fact))).expect("total");
    assert_eq!(store.write(|unit| unit.put(ports::project::<PerStream>(None, &fact))), Ok(()));
}

#[test]
fn a_cleared_read_model_has_no_rows_and_accepts_position_one_again() {
    let store: Subject = subject();
    let fact: Recorded<Counted> = first_fact(&store, "c-1", 5);
    store.write(|unit| unit.put(ports::project::<Total>(None, &fact))).expect("first put");
    store.write(|unit| unit.clear::<Total>()).expect("cleared");
    assert_eq!(store.read(|view| view.rows::<Total>()), Ok(Vec::new()));
    assert_eq!(store.write(|unit| unit.put(ports::project::<Total>(None, &fact))), Ok(()));
}

#[test]
fn clearing_one_read_model_keeps_another() {
    let store: Subject = subject();
    let fact: Recorded<Counted> = first_fact(&store, "c-1", 5);
    store.write(|unit| unit.put(ports::project::<Total>(None, &fact))).expect("total");
    store.write(|unit| unit.put(ports::project::<PerStream>(None, &fact))).expect("per stream");
    store.write(|unit| unit.clear::<Total>()).expect("cleared");
    assert_eq!(store.read(|view| view.rows::<PerStream>()), Ok(vec![("c-1".to_string(), 5)]));
}
