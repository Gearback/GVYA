//! GVYA canonical build-time compiler.
//!
//! compiler/artifact layer owns transparent source resolution, deterministic package composition/audit,
//! canonical runtime IR, and the single compiled `.gvya` artifact format. Runtime SDK/loader
//! integration remains runtime/SDK layer scope.

#![forbid(unsafe_code)]

pub mod analysis;
pub mod artifact;
pub mod audit;
pub mod authoring;
pub mod canonical;
pub mod change;
pub mod ir;
pub mod package;
pub mod pipeline;
pub mod schema_compile;
pub mod source;
pub mod testing;

/// Identifies the implemented compiler-side stage.
#[must_use]
pub const fn foundation_stage() -> &'static str {
    "compiler-artifact"
}
