//! Runtime type registry for the ECS.
//!
//! Every component type used in the ECS is registered here.  The registry
//! assigns each type a stable [`Type`] handle that carries layout, name,
//! and (for non-`Copy` types) a type-erased destructor.

pub(crate) mod registry;
pub use registry::*;
