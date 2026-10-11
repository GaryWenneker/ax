use crate::entry::Entry;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoidError {
    Unknown,
    AlreadyVoided,
}

#[derive(Debug, Default)]
pub struct Ledger {
    entries: Vec<Entry>,
    next_id: u64,
}

impl Ledger {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            next_id: 1,
        }
    }

    pub fn apply(&mut self, amount_cents: i64) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.entries.push(Entry {
            id,
            amount_cents,
            voided: false,
        });
        id
    }

    pub fn void(&mut self, id: u64) -> Result<(), VoidError> {
        let Some(entry) = self.entries.iter_mut().find(|entry| entry.id == id) else {
            return Err(VoidError::Unknown);
        };
        if entry.voided {
            return Err(VoidError::AlreadyVoided);
        }
        entry.voided = true;
        Ok(())
    }

    pub fn balance_cents(&self) -> i64 {
        self.entries
            .iter()
            .filter(|entry| !entry.voided)
            .map(|entry| entry.amount_cents)
            .sum()
    }

    pub fn get(&self, id: u64) -> Option<&Entry> {
        self.entries.iter().find(|entry| entry.id == id)
    }
}
