//! Pi execution stays in Pi. This crate observes that run and advises Ax context.

mod context;
mod economics;
mod events;
mod integration;
mod report;
mod store;
mod types;

pub use context::ContextCache;
pub use economics::display_amount;
pub use integration::{
    create_pi_integration, note_read_guard, read_economics, read_optimization, PiError,
    PiIntegration, PiOptions,
};
pub use report::apply_display_currency;
pub use report::{
    format_economics, format_optimization, EconomicsReport, OptimizationGroup, OptimizationReport,
};
pub use types::*;
