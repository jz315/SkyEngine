//! SkyEngine reflection facade.
//!
//! The high-level inspector reflection API lives in `sky_reflect`. This module
//! preserves the `sky_engine::reflect` entry point and exposes reflection
//! helpers for SkyEngine-owned built-in types.

pub use sky_reflect::*;

/// Register SkyEngine-owned reflected types on top of the core built-ins.
pub fn register_engine_builtins(
    registry: &mut ReflectRegistry,
) -> Result<&mut ReflectRegistry, ReflectError> {
    sky_math::reflect::register_builtins(registry)
}

/// Create a reflection registry with core and SkyEngine-owned built-ins.
pub fn registry_with_engine_builtins() -> ReflectRegistry {
    sky_math::reflect::registry_with_builtins()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::{Quat, Transform, Vec2, Vec3, Vec4};

    #[derive(Reflect)]
    #[reflect(name = "game.FacadeStats")]
    struct FacadeStats {
        #[reflect(label = "HP", min = 0, max = 100, step = 1)]
        hp: u32,
    }

    #[test]
    fn core_builtins_do_not_include_engine_math_types() {
        let registry = ReflectRegistry::with_builtins();
        assert!(registry.type_of::<Vec3>().is_none());
    }

    #[test]
    fn engine_builtins_register_math_and_transform_types() {
        let registry = registry_with_engine_builtins();
        assert!(registry.type_of::<Vec2>().is_some());
        assert!(registry.type_of::<Vec3>().is_some());
        assert!(registry.type_of::<Vec4>().is_some());
        assert!(registry.type_of::<Quat>().is_some());
        assert!(registry.type_of::<Transform>().is_some());
    }

    #[test]
    fn transform_reflection_reads_and_writes_engine_values() {
        let registry = registry_with_engine_builtins();
        let mut transform = Transform::from_xyz(1.0, 2.0, 3.0);

        let position = ReflectPath::new("position").unwrap();
        assert_eq!(
            position.read(&registry, &transform).unwrap(),
            ReflectValue::Vec3([1.0, 2.0, 3.0])
        );
        position
            .write(
                &registry,
                &mut transform,
                ReflectValue::Vec3([4.0, 5.0, 6.0]),
            )
            .unwrap();
        assert_eq!(transform.position.to_array(), [4.0, 5.0, 6.0]);
    }

    #[test]
    fn reflect_derive_works_through_facade_reexport() {
        let mut registry = ReflectRegistry::with_builtins();
        registry.register::<FacadeStats>().unwrap();

        let ty = registry.type_of::<FacadeStats>().unwrap();
        let hp = ty.field("hp").unwrap();
        assert_eq!(ty.path(), "game.FacadeStats");
        assert_eq!(hp.attrs().label.as_deref(), Some("HP"));
        assert_eq!(hp.attrs().min, Some(0.0));
        assert_eq!(hp.attrs().max, Some(100.0));
        assert_eq!(hp.attrs().step, Some(1.0));
    }
}
