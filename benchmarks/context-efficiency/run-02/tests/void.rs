use bench_ledger::{Ledger, VoidError};

#[test]
fn void_unknown_id_does_not_change_balance() {
    let mut ledger = Ledger::new();
    ledger.apply(100);
    assert_eq!(ledger.balance_cents(), 100);
    assert_eq!(ledger.void(99), Err(VoidError::Unknown));
    assert_eq!(ledger.balance_cents(), 100);
    assert!(!ledger.get(1).unwrap().voided);
}

#[test]
fn void_twice_returns_already_voided() {
    let mut ledger = Ledger::new();
    let id = ledger.apply(80);
    assert_eq!(ledger.void(id), Ok(()));
    assert_eq!(ledger.balance_cents(), 0);
    assert!(ledger.get(id).unwrap().voided);
    assert_eq!(ledger.void(id), Err(VoidError::AlreadyVoided));
    assert_eq!(ledger.balance_cents(), 0);
    assert!(ledger.get(id).unwrap().voided);
}

#[test]
fn void_one_of_two_leaves_the_other_in_the_balance() {
    let mut ledger = Ledger::new();
    let first = ledger.apply(150);
    let second = ledger.apply(-40);
    assert_eq!(first, 1);
    assert_eq!(second, 2);
    assert_eq!(ledger.void(first), Ok(()));
    assert_eq!(ledger.balance_cents(), -40);
    assert!(ledger.get(first).unwrap().voided);
    assert!(!ledger.get(second).unwrap().voided);
}
