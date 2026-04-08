//! Cranelift codegen — lower CIR IR to native code.
//!
//! All submodules are gated behind the `codegen` Cargo feature.
//!
//! Pipeline:
//!   1. `lower_module` — translate CIR ops to Cranelift CLIF, produce object bytes
//!   2. `write_object_file` — write those bytes to disk
//!   3. `link_executable` — invoke `cc` to link into a native executable

#[cfg(feature = "codegen")]
pub mod types;
#[cfg(feature = "codegen")]
mod lower;
#[cfg(feature = "codegen")]
mod emit;
#[cfg(feature = "codegen")]
mod link;
#[cfg(feature = "codegen")]
pub mod construct_lowering;
#[cfg(feature = "codegen")]
pub mod lowering_ctx;

#[cfg(feature = "codegen")]
pub use lower::lower_module;
#[cfg(feature = "codegen")]
pub use emit::write_object_file;
#[cfg(feature = "codegen")]
pub use link::link_executable;
#[cfg(feature = "codegen")]
pub use construct_lowering::{ConstructLowering, LoweringRegistry};
#[cfg(feature = "codegen")]
pub use lowering_ctx::LoweringCtx;
