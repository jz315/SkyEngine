//! Inspector reflection metadata built on the shared `sky_type` foundation.
//!
//! `sky_type` owns runtime type identity, layout, and drop semantics. This
//! crate owns the higher-level [`Reflect`] trait, [`ReflectRegistry`],
//! [`ReflectField`] metadata, and [`ReflectValue`] edit/snapshot values used by
//! inspectors and tooling.

mod registry;
mod value;

pub use registry::*;
pub use sky_reflect_derive::Reflect;
pub use value::*;
