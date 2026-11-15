#[test]
fn a_commit_tells_the_listener_the_position_of_its_last_fact() {
    let told: std::sync::Arc<std::sync::Mutex<Vec<Position>>> = std::sync::Arc::default();
    let listener: std::sync::Arc<std::sync::Mutex<Vec<Position>>> = std::sync::Arc::clone(&told);
    let store: Subject = subject().on_commit(move |position: Position| listener.lock().expect("lock").push(position));
    add(&store, "c-1", 1);
    let _twice: Applied<Counter> = store
        .write(|unit| unit.apply(&id("c-1"), |state: Counter| ports::decide(AddTwice { by: 3 }, state)))
        .expect("applied");
    assert_eq!(*told.lock().expect("lock"), vec![Position(1), Position(3)]);
}

#[test]
fn a_unit_that_appends_no_fact_tells_the_listener_nothing() {
    let told: std::sync::Arc<std::sync::Mutex<Vec<Position>>> = std::sync::Arc::default();
    let listener: std::sync::Arc<std::sync::Mutex<Vec<Position>>> = std::sync::Arc::clone(&told);
    let store: Subject = subject().on_commit(move |position: Position| listener.lock().expect("lock").push(position));
    add(&store, "c-1", 0);
    let _failed: Result<(), StoreError> = store.write(|unit| {
        let _applied: Applied<Counter> = unit.apply(&id("c-1"), |state: Counter| ports::decide(Add { by: 1 }, state))?;
        Err(StoreError::Unavailable { cause: "injected".to_string() })
    });
    assert_eq!(*told.lock().expect("lock"), Vec::new());
}

#[test]
fn a_unit_reads_the_state_it_wrote() {
    let store: Subject = subject();
    let versions: (Version, Version) = store
        .write(|unit| {
            let first: Applied<Counter> = unit.apply(&id("c-1"), |state: Counter| ports::decide(Add { by: 1 }, state))?;
            let second: Applied<Counter> = unit.apply(&id("c-1"), |state: Counter| ports::decide(Add { by: 2 }, state))?;
            Ok((first.facts[0].version(), second.facts[0].version()))
        })
        .expect("applied twice");
    assert_eq!(versions, (Version(1), Version(2)));
    assert_eq!(counter(&store, "c-1"), (Version(2), Counter::Holding { total: 3 }));
}

#[test]
fn a_unit_reads_the_facts_and_rows_it_wrote() {
    let store: Subject = subject();
    let read: (usize, Option<u64>) = store
        .write(|unit| {
            let applied: Applied<Counter> = unit.apply(&id("c-1"), |state: Counter| ports::decide(Add { by: 4 }, state))?;
            unit.put(ports::project::<Total>(None, &applied.facts[0]))?;
            Ok((unit.facts::<Counter>(Position(0), 10)?.len(), unit.row::<Total>("all")?))
        })
        .expect("unit");
    assert_eq!(read, (1, Some(4)));
}

#[test]
fn a_unit_that_clears_and_puts_leaves_only_what_it_put() {
    let store: Subject = subject();
    let fact: Recorded<Counted> = first_fact(&store, "c-1", 5);
    store.write(|unit| unit.put(ports::project::<PerStream>(None, &fact))).expect("put");
    let other: Recorded<Counted> = first_fact(&store, "c-2", 2);
    store
        .write(|unit| {
            unit.clear::<PerStream>()?;
            unit.put(ports::project::<PerStream>(None, &other))
        })
        .expect("cleared and put");
    assert_eq!(store.read(|view| view.rows::<PerStream>()), Ok(vec![("c-2".to_string(), 2)]));
}

#[test]
fn a_unit_that_fails_stores_no_state_fact_row_or_request() {
    let store: Subject = subject();
    let failed: Result<(), StoreError> = store.write(|unit| {
        let applied: Applied<Counter> = unit.apply(&id("c-1"), |state: Counter| ports::decide(Add { by: 1 }, state))?;
        unit.put(ports::project::<Total>(None, &applied.facts[0]))?;
        let _sent: Applied<Courier> = unit.apply(&id("p-1"), |state: Courier| ports::react(Send { number: 1 }, state))?;
        Err(StoreError::Unavailable { cause: "injected".to_string() })
    });
    assert_eq!(failed, Err(StoreError::Unavailable { cause: "injected".to_string() }));
    assert_eq!(counter(&store, "c-1"), (Version(0), Counter::Empty));
    assert_eq!(counter_facts(&store, 0, 10), Vec::new());
    assert_eq!(store.read(|view| view.rows::<Total>()), Ok(Vec::new()));
    assert_eq!(parcels(&store, 10), Vec::new());
}

#[test]
fn a_fact_after_a_failed_unit_takes_the_next_free_version() {
    let store: Subject = subject();
    let _failed: Result<(), StoreError> = store.write(|unit| {
        let _applied: Applied<Counter> = unit.apply(&id("c-1"), |state: Counter| ports::decide(Add { by: 1 }, state))?;
        Err(StoreError::Unavailable { cause: "injected".to_string() })
    });
    let facts: Vec<Recorded<Counted>> = add(&store, "c-1", 2);
    assert_eq!((facts[0].stream().clone(), facts[0].version()), (id("c-1"), Version(1)));
}

#[test]
fn a_unit_that_panics_stores_nothing_and_leaves_the_store_usable() {
    let store: Subject = subject();
    let panicked: std::thread::Result<Result<(), StoreError>> = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        store.write(|unit| {
            let _applied: Applied<Counter> = unit.apply(&id("c-1"), |state: Counter| ports::decide(Add { by: 1 }, state))?;
            panic!("a bug inside a unit of work")
        })
    }));
    assert!(panicked.is_err());
    assert_eq!(counter(&store, "c-1"), (Version(0), Counter::Empty));
    assert_eq!(add(&store, "c-2", 1).len(), 1);
}
