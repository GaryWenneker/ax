//! Test-Impact Analysis: reverse reachability from dirty nodes to tests.

mod options;
mod reachability;

pub use options::TiaOptions;
pub use reachability::{affected_files_from_changes, find_impacted_tests, ImpactedTest, TiaResult};
