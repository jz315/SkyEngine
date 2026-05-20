//! Inspector reflection metadata built on the shared `sky_type` foundation.
//!
//! ECS and tools share the thin [`Type`] layout layer for type identity,
//! component storage, and drop semantics. Tools can opt into the higher-level
//! [`Reflect`] derive, [`ReflectRegistry`], and [`ReflectField`] metadata for
//! inspector-style editing.

mod registry;
mod value;

pub use registry::*;
pub use sky_engine_reflect_derive::Reflect;
pub use value::*;
