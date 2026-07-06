//! Test-Impact Analysis: reverse reachability from dirty nodes to tests.

mod reachability;
mod options;

pub use reachability::{affected_files_from_changes, find_impacted_tests, ImpactedTest, TiaResult};
pub use options::TiaOptions;
