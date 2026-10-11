#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub id: u64,
    pub amount_cents: i64,
    pub voided: bool,
}
