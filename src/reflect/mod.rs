//! Foundational runtime reflection plus inspector metadata.
//!
//! ECS uses the thin [`Type`] layout layer for component storage and drop
//! semantics. Tools can opt into the higher-level [`Reflect`] derive,
//! [`ReflectRegistry`], and [`ReflectField`] metadata for inspector-style
//! editing.

mod registry;
mod value;

pub use registry::*;
pub use sky_engine_reflect_derive::Reflect;
pub use value::*;
