use bench_ledger::{Ledger, VoidError};

#[test]
fn void_unknown_id_does_not_change_balance() {
    let mut ledger = Ledger::new();
    ledger.apply(100);
    let before = ledger.balance_cents();

    let err = ledger.void(99).unwrap_err();

    assert_eq!(err, VoidError::Unknown);
    assert_eq!(ledger.balance_cents(), before);
}

#[test]
fn void_twice_returns_already_voided() {
    let mut ledger = Ledger::new();
    let id = ledger.apply(80);
    ledger.void(id).unwrap();
    assert_eq!(ledger.balance_cents(), 0);

    let err = ledger.void(id).unwrap_err();

    assert_eq!(err, VoidError::AlreadyVoided);
    assert_eq!(ledger.balance_cents(), 0);
    assert!(ledger.get(id).unwrap().voided);
}

#[test]
fn void_one_of_two_leaves_the_other_in_balance() {
    let mut ledger = Ledger::new();
    let first = ledger.apply(150);
    let second = ledger.apply(-40);
    assert_eq!(first, 1);
    assert_eq!(second, 2);

    ledger.void(first).unwrap();

    assert!(ledger.get(first).unwrap().voided);
    assert!(!ledger.get(second).unwrap().voided);
    assert_eq!(ledger.balance_cents(), -40);
}
