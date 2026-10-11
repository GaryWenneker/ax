use bench_ledger::Ledger;

#[test]
fn apply_updates_balance_and_ids() {
    let mut ledger = Ledger::new();
    let first = ledger.apply(150);
    let second = ledger.apply(-40);
    assert_eq!(first, 1);
    assert_eq!(second, 2);
    assert_eq!(ledger.balance_cents(), 110);
    assert_eq!(ledger.get(1).unwrap().amount_cents, 150);
}
