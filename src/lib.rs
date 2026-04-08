pub mod codegen;
pub mod diagnostic;
pub mod entity;
pub mod ir;
pub mod pass;
pub mod transform;
pub mod verify;

// Re-export entity IDs and arena infrastructure.
pub use entity::{BlockId, EntityRef, OpId, PrimaryMap, RegionId, TypeId, ValueId};

// Re-export IR core types.
pub use ir::attributes::{Attribute, NamedAttribute};
pub use ir::block::{BlockData, BlockOpIter};
pub use ir::builder::{Builder, InsertionPoint};
pub use ir::context::Context;
pub use ir::dialect::{Dialect, OpDefinition, OpTrait};
pub use ir::location::Location;
pub use ir::module::Module;
pub use ir::operation::OperationData;
pub use ir::region::RegionData;
pub use ir::symbol_table::SymbolTable;
pub use ir::types::{ExtensionType, TypeKind};
pub use ir::value::{Use, ValueData, ValueKind};

// Re-export diagnostics.
pub use diagnostic::diagnostic::{Diagnostic, DiagnosticError, Severity};
pub use diagnostic::handler::DiagnosticHandler;

// Re-export pass infrastructure.
pub use pass::external::{create_external_pass, ExternalPass, ExternalPassCallbacks};
pub use pass::manager::PassManager;
pub use pass::pass::Pass;

// Re-export transform types (implementations are on Context).
pub use transform::walk::{WalkOrder, WalkResult};

// Re-export verification.
pub use verify::verifier::verify;
