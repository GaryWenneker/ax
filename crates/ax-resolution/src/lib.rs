//! Symbol resolution for ax.

pub mod callback_synthesizer;
pub mod c_fnptr_synthesizer;
pub mod framework_resolve;
pub mod frameworks;
pub mod import_resolver;
pub mod name_matcher;
pub mod resolver;
pub mod strip_comments;
pub mod types;

pub use resolver::ReferenceResolver;
pub use types::*;
