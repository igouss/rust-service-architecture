use ports::Position;

#[test]
fn an_id_never_passes_the_last_integer_a_store_holds() {
    assert_eq!(Position::ZERO.after(u64::MAX).get(), i64::MAX.unsigned_abs());
}

#[test]
fn a_stored_integer_loads_as_the_same_id() {
    assert_eq!(Position::loaded(5), Some(Position::from(5)));
}

#[test]
fn a_negative_stored_integer_loads_as_no_id() {
    assert_eq!(Position::loaded(-1), None);
}
