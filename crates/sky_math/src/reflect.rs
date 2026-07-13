use std::any::{type_name, Any};

use sky_reflect::{
    type_name_of_any, type_name_of_any_mut, Reflect, ReflectAttrs, ReflectError, ReflectField,
    ReflectRegistry, ReflectStructValue, ReflectType, ReflectValue,
};

use crate::{
    Affine2, Affine3, Color, IVec2, IVec3, Mat3, Mat4, Quat, Srgba, Transform, UVec2, UVec3, Vec2,
    Vec3, Vec4,
};

/// Register Sky math reflected types on top of the core reflect built-ins.
pub fn register_builtins(
    registry: &mut ReflectRegistry,
) -> Result<&mut ReflectRegistry, ReflectError> {
    registry.register::<Vec2>()?;
    registry.register::<Vec3>()?;
    registry.register::<Vec4>()?;
    registry.register::<Quat>()?;
    registry.register::<Transform>()?;
    registry.register::<Color>()?;
    registry.register::<Srgba>()?;
    registry.register::<IVec2>()?;
    registry.register::<IVec3>()?;
    registry.register::<UVec2>()?;
    registry.register::<UVec3>()?;
    registry.register::<Mat3>()?;
    registry.register::<Mat4>()?;
    registry.register::<Affine2>()?;
    registry.register::<Affine3>()?;
    Ok(registry)
}

macro_rules! reflect_f32_array {
    ($ty:ty, $path:literal, $len:expr) => {
        impl Reflect for $ty {
            fn reflect_type() -> ReflectType {
                ReflectType::new_value::<Self>($path)
            }
            fn to_reflect_value(&self) -> Result<ReflectValue, ReflectError> {
                Ok(ReflectValue::Array(
                    self.to_cols_array()
                        .into_iter()
                        .map(ReflectValue::F32)
                        .collect(),
                ))
            }
            fn apply_reflect_value(&mut self, value: ReflectValue) -> Result<(), ReflectError> {
                let ReflectValue::Array(values) = value else {
                    return Err(value_mismatch("Array", &value));
                };
                if values.len() != $len {
                    return Err(ReflectError::Unsupported {
                        message: format!("{} requires {} values", $path, $len),
                    });
                }
                let mut output = [0.0f32; $len];
                for (index, value) in values.into_iter().enumerate() {
                    let ReflectValue::F32(value) = value else {
                        return Err(value_mismatch("f32", &value));
                    };
                    output[index] = value;
                }
                *self = Self::from_cols_array(output);
                Ok(())
            }
        }
    };
}

macro_rules! reflect_integer_vector {
    ($ty:ty, $path:literal, $len:expr, $variant:ident, $scalar:ty) => {
        impl Reflect for $ty {
            fn reflect_type() -> ReflectType {
                ReflectType::new_value::<Self>($path)
            }
            fn to_reflect_value(&self) -> Result<ReflectValue, ReflectError> {
                Ok(ReflectValue::Array(
                    self.to_array()
                        .into_iter()
                        .map(|value| ReflectValue::$variant(value as _))
                        .collect(),
                ))
            }
            fn apply_reflect_value(&mut self, value: ReflectValue) -> Result<(), ReflectError> {
                let ReflectValue::Array(values) = value else {
                    return Err(value_mismatch("Array", &value));
                };
                if values.len() != $len {
                    return Err(ReflectError::Unsupported {
                        message: format!("{} requires {} values", $path, $len),
                    });
                }
                let mut output = [0 as $scalar; $len];
                for (index, value) in values.into_iter().enumerate() {
                    let ReflectValue::$variant(value) = value else {
                        return Err(value_mismatch(stringify!($variant), &value));
                    };
                    output[index] = <$scalar>::try_from(value).map_err(|_| {
                        ReflectError::IntegerOutOfRange {
                            target: stringify!($scalar),
                            value: value.to_string(),
                        }
                    })?;
                }
                *self = Self::from_array(output);
                Ok(())
            }
        }
    };
}

macro_rules! reflect_color {
    ($ty:ty, $path:literal) => {
        impl Reflect for $ty {
            fn reflect_type() -> ReflectType {
                ReflectType::new_value::<Self>($path)
            }
            fn to_reflect_value(&self) -> Result<ReflectValue, ReflectError> {
                Ok(ReflectValue::Vec4(self.to_array()))
            }
            fn apply_reflect_value(&mut self, value: ReflectValue) -> Result<(), ReflectError> {
                match value {
                    ReflectValue::Vec4(value) => {
                        *self = Self::from(value);
                        Ok(())
                    }
                    other => Err(value_mismatch("Vec4", &other)),
                }
            }
        }
    };
}

reflect_color!(Color, "sky.Color");
reflect_color!(Srgba, "sky.Srgba");
reflect_integer_vector!(IVec2, "sky.IVec2", 2, I64, i32);
reflect_integer_vector!(IVec3, "sky.IVec3", 3, I64, i32);
reflect_integer_vector!(UVec2, "sky.UVec2", 2, U64, u32);
reflect_integer_vector!(UVec3, "sky.UVec3", 3, U64, u32);
reflect_f32_array!(Mat3, "sky.Mat3", 9);
reflect_f32_array!(Mat4, "sky.Mat4", 16);
reflect_f32_array!(Affine2, "sky.Affine2", 6);
reflect_f32_array!(Affine3, "sky.Affine3", 12);

/// Create a reflection registry with core and Sky math built-ins.
pub fn registry_with_builtins() -> ReflectRegistry {
    let mut registry = ReflectRegistry::with_builtins();
    register_builtins(&mut registry).expect("Sky math reflect built-ins must register");
    registry
}

impl Reflect for Vec2 {
    fn reflect_type() -> ReflectType {
        ReflectType::new_value::<Self>("sky.Vec2")
    }

    fn to_reflect_value(&self) -> Result<ReflectValue, ReflectError> {
        Ok(ReflectValue::Vec2(self.to_array()))
    }

    fn apply_reflect_value(&mut self, value: ReflectValue) -> Result<(), ReflectError> {
        match value {
            ReflectValue::Vec2(value) => {
                *self = Vec2::from_array(value);
                Ok(())
            }
            other => Err(value_mismatch("Vec2", &other)),
        }
    }
}

impl Reflect for Vec3 {
    fn reflect_type() -> ReflectType {
        ReflectType::new_value::<Self>("sky.Vec3")
    }

    fn to_reflect_value(&self) -> Result<ReflectValue, ReflectError> {
        Ok(ReflectValue::Vec3(self.to_array()))
    }

    fn apply_reflect_value(&mut self, value: ReflectValue) -> Result<(), ReflectError> {
        match value {
            ReflectValue::Vec3(value) => {
                *self = Vec3::from_array(value);
                Ok(())
            }
            other => Err(value_mismatch("Vec3", &other)),
        }
    }
}

impl Reflect for Vec4 {
    fn reflect_type() -> ReflectType {
        ReflectType::new_value::<Self>("sky.Vec4")
    }

    fn to_reflect_value(&self) -> Result<ReflectValue, ReflectError> {
        Ok(ReflectValue::Vec4(self.to_array()))
    }

    fn apply_reflect_value(&mut self, value: ReflectValue) -> Result<(), ReflectError> {
        match value {
            ReflectValue::Vec4(value) => {
                *self = Vec4::from_array(value);
                Ok(())
            }
            other => Err(value_mismatch("Vec4", &other)),
        }
    }
}

impl Reflect for Quat {
    fn reflect_type() -> ReflectType {
        ReflectType::new_value::<Self>("sky.Quat")
    }

    fn to_reflect_value(&self) -> Result<ReflectValue, ReflectError> {
        Ok(ReflectValue::Quat(self.to_xyzw_array()))
    }

    fn apply_reflect_value(&mut self, value: ReflectValue) -> Result<(), ReflectError> {
        match value {
            ReflectValue::Quat(value) => {
                *self = Quat::from_xyzw_array(value);
                Ok(())
            }
            other => Err(value_mismatch("Quat", &other)),
        }
    }
}

impl Reflect for Transform {
    fn reflect_type() -> ReflectType {
        ReflectType::new_struct::<Self>(
            "sky.Transform",
            vec![
                ReflectField::new_raw::<Self, Vec3>(
                    "position",
                    ReflectAttrs::default(),
                    |owner| {
                        let owner = owner.downcast_ref::<Transform>().ok_or_else(|| {
                            ReflectError::OwnerTypeMismatch {
                                expected: type_name::<Transform>().to_string(),
                                actual: type_name_of_any(owner).to_string(),
                            }
                        })?;
                        owner.position.to_reflect_value()
                    },
                    |owner, value| {
                        let actual = type_name_of_any_mut(owner).to_string();
                        let owner = owner.downcast_mut::<Transform>().ok_or_else(|| {
                            ReflectError::OwnerTypeMismatch {
                                expected: type_name::<Transform>().to_string(),
                                actual,
                            }
                        })?;
                        owner.position.apply_reflect_value(value)
                    },
                    |owner| {
                        let owner = owner.downcast_ref::<Transform>().ok_or_else(|| {
                            ReflectError::OwnerTypeMismatch {
                                expected: type_name::<Transform>().to_string(),
                                actual: type_name_of_any(owner).to_string(),
                            }
                        })?;
                        Ok(&owner.position as &dyn Any)
                    },
                    |owner| {
                        let actual = type_name_of_any_mut(owner).to_string();
                        let owner = owner.downcast_mut::<Transform>().ok_or_else(|| {
                            ReflectError::OwnerTypeMismatch {
                                expected: type_name::<Transform>().to_string(),
                                actual,
                            }
                        })?;
                        Ok(&mut owner.position as &mut dyn Any)
                    },
                ),
                ReflectField::new_raw::<Self, Vec3>(
                    "scale",
                    ReflectAttrs::default(),
                    |owner| {
                        let owner = owner.downcast_ref::<Transform>().ok_or_else(|| {
                            ReflectError::OwnerTypeMismatch {
                                expected: type_name::<Transform>().to_string(),
                                actual: type_name_of_any(owner).to_string(),
                            }
                        })?;
                        owner.scale.to_reflect_value()
                    },
                    |owner, value| {
                        let actual = type_name_of_any_mut(owner).to_string();
                        let owner = owner.downcast_mut::<Transform>().ok_or_else(|| {
                            ReflectError::OwnerTypeMismatch {
                                expected: type_name::<Transform>().to_string(),
                                actual,
                            }
                        })?;
                        owner.scale.apply_reflect_value(value)
                    },
                    |owner| {
                        let owner = owner.downcast_ref::<Transform>().ok_or_else(|| {
                            ReflectError::OwnerTypeMismatch {
                                expected: type_name::<Transform>().to_string(),
                                actual: type_name_of_any(owner).to_string(),
                            }
                        })?;
                        Ok(&owner.scale as &dyn Any)
                    },
                    |owner| {
                        let actual = type_name_of_any_mut(owner).to_string();
                        let owner = owner.downcast_mut::<Transform>().ok_or_else(|| {
                            ReflectError::OwnerTypeMismatch {
                                expected: type_name::<Transform>().to_string(),
                                actual,
                            }
                        })?;
                        Ok(&mut owner.scale as &mut dyn Any)
                    },
                ),
                ReflectField::new_raw::<Self, Quat>(
                    "rotation",
                    ReflectAttrs::default(),
                    |owner| {
                        let owner = owner.downcast_ref::<Transform>().ok_or_else(|| {
                            ReflectError::OwnerTypeMismatch {
                                expected: type_name::<Transform>().to_string(),
                                actual: type_name_of_any(owner).to_string(),
                            }
                        })?;
                        owner.rotation.to_reflect_value()
                    },
                    |owner, value| {
                        let actual = type_name_of_any_mut(owner).to_string();
                        let owner = owner.downcast_mut::<Transform>().ok_or_else(|| {
                            ReflectError::OwnerTypeMismatch {
                                expected: type_name::<Transform>().to_string(),
                                actual,
                            }
                        })?;
                        owner.rotation.apply_reflect_value(value)
                    },
                    |owner| {
                        let owner = owner.downcast_ref::<Transform>().ok_or_else(|| {
                            ReflectError::OwnerTypeMismatch {
                                expected: type_name::<Transform>().to_string(),
                                actual: type_name_of_any(owner).to_string(),
                            }
                        })?;
                        Ok(&owner.rotation as &dyn Any)
                    },
                    |owner| {
                        let actual = type_name_of_any_mut(owner).to_string();
                        let owner = owner.downcast_mut::<Transform>().ok_or_else(|| {
                            ReflectError::OwnerTypeMismatch {
                                expected: type_name::<Transform>().to_string(),
                                actual,
                            }
                        })?;
                        Ok(&mut owner.rotation as &mut dyn Any)
                    },
                ),
            ],
        )
    }

    fn reflect_dependencies(registry: &mut ReflectRegistry) -> Result<(), ReflectError> {
        registry.register::<Vec3>()?;
        registry.register::<Quat>()?;
        Ok(())
    }

    fn to_reflect_value(&self) -> Result<ReflectValue, ReflectError> {
        Ok(ReflectValue::Struct(
            ReflectStructValue::new("sky.Transform")
                .with_field("position", self.position.to_reflect_value()?)
                .with_field("scale", self.scale.to_reflect_value()?)
                .with_field("rotation", self.rotation.to_reflect_value()?),
        ))
    }

    fn apply_reflect_value(&mut self, value: ReflectValue) -> Result<(), ReflectError> {
        let ReflectValue::Struct(value) = value else {
            return Err(value_mismatch("Struct", &value));
        };

        for field in value.fields() {
            match field.name() {
                "position" => self.position.apply_reflect_value(field.value().clone())?,
                "scale" => self.scale.apply_reflect_value(field.value().clone())?,
                "rotation" => self.rotation.apply_reflect_value(field.value().clone())?,
                name => {
                    return Err(ReflectError::UnknownField {
                        type_name: "sky.Transform".to_string(),
                        field: name.to_string(),
                    });
                }
            }
        }
        Ok(())
    }
}

fn value_mismatch(expected: &'static str, value: &ReflectValue) -> ReflectError {
    ReflectError::ValueTypeMismatch {
        expected,
        actual: value.kind_name(),
    }
}
