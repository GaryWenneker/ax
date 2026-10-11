Add `Ledger::void` to the bench-ledger crate in this directory.

Required behavior:

- `void(id)` returns `Result<(), VoidError>`.
- Voiding an open entry sets it voided and removes its amount from `balance_cents`.
- Voiding an unknown id returns `Err(VoidError::Unknown)` and does not change the balance.
- Voiding an id that is already voided returns `Err(VoidError::AlreadyVoided)` and does not change the balance.
- `apply` still assigns ids starting at 1.
- Add `tests/void.rs` covering unknown id, double void, and voiding one of two entries so the other remains in the balance.
- Do not change `tests/apply.rs`.
- Run `cargo test` in this directory and leave it passing.
- Edit only files inside this directory.
