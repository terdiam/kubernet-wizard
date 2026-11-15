//! Core domain for kubernet-wizard: cluster spec, validation, compatibility,
//! presets, HA upgrade planning and footprint estimation.

pub mod caps;
pub mod compat;
pub mod ha;
pub mod presets;
pub mod resources;
pub mod spec;
pub mod validate;

pub use caps::{Capabilities, Phase};
pub use spec::*;
pub use validate::{Field, Issue, Report, Severity, Step, validate};
