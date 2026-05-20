use rustc_hash::FxHashMap;
use std::any::{type_name, Any, TypeId};

use crate::math::{Quat, Transform, Vec2, Vec3, Vec4};

use super::{ReflectError, ReflectStructValue, ReflectValue};
pub use sky_type::{
    query_by_name, query_by_rust_type, register, registered_types, type_of, Type, TypeInfo,
};

/// Best-effort name for erased values. `std::any::Any` exposes `TypeId`, not a
/// stable dynamic type name, so this is only used in diagnostics.
pub fn type_name_of_any(_: &dyn Any) -> &'static str {
    "<erased>"
}

/// Best-effort name for erased mutable values.
pub fn type_name_of_any_mut(_: &mut dyn Any) -> &'static str {
    "<erased>"
}

/// ECS-facing semantic aliases for the shared foundational type layer.
pub type ComponentType = Type;

pub fn component_type<T: 'static>() -> ComponentType {
    type_of::<T>()
}

pub fn register_component_type(name: &str, size: usize, align: usize) -> ComponentType {
    register(name, size, align)
}

pub fn component_type_by_name(name: &str) -> Option<ComponentType> {
    query_by_name(name)
}

pub fn component_type_by_rust_type<T: 'static>() -> Option<ComponentType> {
    query_by_rust_type::<T>()
}

pub fn registered_component_types() -> Vec<ComponentType> {
    registered_types()
}

/// Derive-able trait for inspector reflection.
pub trait Reflect: Any + 'static {
    fn reflect_type() -> ReflectType;

    fn reflect_dependencies(_registry: &mut ReflectRegistry) -> Result<(), ReflectError> {
        Ok(())
    }

    fn to_reflect_value(&self) -> Result<ReflectValue, ReflectError>;

    fn apply_reflect_value(&mut self, value: ReflectValue) -> Result<(), ReflectError>;
}

/// Inspector type category.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReflectKind {
    Value,
    Struct,
    Enum,
    Option,
    List,
    Array,
    Opaque,
}

/// Inspector metadata attached to a reflected field.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReflectAttrs {
    pub label: Option<String>,
    pub readonly: bool,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub step: Option<f64>,
    pub category: Option<String>,
}

type ReadFieldFn = fn(&dyn Any) -> Result<ReflectValue, ReflectError>;
type WriteFieldFn = fn(&mut dyn Any, ReflectValue) -> Result<(), ReflectError>;
type GetFieldFn = for<'a> fn(&'a dyn Any) -> Result<&'a dyn Any, ReflectError>;
type GetFieldMutFn = for<'a> fn(&'a mut dyn Any) -> Result<&'a mut dyn Any, ReflectError>;

/// Field metadata plus read/write callbacks.
#[derive(Clone)]
pub struct ReflectField {
    name: String,
    type_id: TypeId,
    type_path: String,
    attrs: ReflectAttrs,
    read: ReadFieldFn,
    write: WriteFieldFn,
    get: GetFieldFn,
    get_mut: GetFieldMutFn,
}

impl ReflectField {
    pub fn new_raw<Owner, Field>(
        name: impl Into<String>,
        attrs: ReflectAttrs,
        read: ReadFieldFn,
        write: WriteFieldFn,
        get: GetFieldFn,
        get_mut: GetFieldMutFn,
    ) -> Self
    where
        Owner: Any + 'static,
        Field: Reflect,
    {
        let field_ty = Field::reflect_type();
        Self {
            name: name.into(),
            type_id: TypeId::of::<Field>(),
            type_path: field_ty.path().to_string(),
            attrs,
            read,
            write,
            get,
            get_mut,
        }
    }

    #[inline]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[inline]
    pub fn type_id(&self) -> TypeId {
        self.type_id
    }

    #[inline]
    pub fn type_path(&self) -> &str {
        &self.type_path
    }

    #[inline]
    pub fn attrs(&self) -> &ReflectAttrs {
        &self.attrs
    }

    pub fn read<T: Any>(&self, owner: &T) -> Result<ReflectValue, ReflectError> {
        self.read_any(owner)
    }

    pub fn write<T: Any>(&self, owner: &mut T, value: ReflectValue) -> Result<(), ReflectError> {
        self.write_any(owner, value)
    }

    pub fn read_any(&self, owner: &dyn Any) -> Result<ReflectValue, ReflectError> {
        (self.read)(owner)
    }

    pub fn write_any(&self, owner: &mut dyn Any, value: ReflectValue) -> Result<(), ReflectError> {
        if self.attrs.readonly {
            return Err(ReflectError::ReadonlyField {
                field: self.name.clone(),
            });
        }
        (self.write)(owner, value)
    }

    pub fn get_any<'a>(&self, owner: &'a dyn Any) -> Result<&'a dyn Any, ReflectError> {
        (self.get)(owner)
    }

    pub fn get_any_mut<'a>(&self, owner: &'a mut dyn Any) -> Result<&'a mut dyn Any, ReflectError> {
        if self.attrs.readonly {
            return Err(ReflectError::ReadonlyField {
                field: self.name.clone(),
            });
        }
        (self.get_mut)(owner)
    }
}

impl std::fmt::Debug for ReflectField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReflectField")
            .field("name", &self.name)
            .field("type_path", &self.type_path)
            .field("attrs", &self.attrs)
            .finish()
    }
}

/// Reflected enum variant shape.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReflectVariantKind {
    Unit,
    Tuple { fields: usize },
    Struct { fields: Vec<String> },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReflectVariant {
    name: String,
    kind: ReflectVariantKind,
}

impl ReflectVariant {
    pub fn new(name: impl Into<String>, kind: ReflectVariantKind) -> Self {
        Self {
            name: name.into(),
            kind,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn kind(&self) -> &ReflectVariantKind {
        &self.kind
    }
}

/// Inspector type metadata, distinct from the foundational [`Type`] layout.
#[derive(Clone, Debug)]
pub struct ReflectType {
    layout: Type,
    type_id: TypeId,
    path: String,
    kind: ReflectKind,
    fields: Vec<ReflectField>,
    variants: Vec<ReflectVariant>,
}

impl ReflectType {
    pub fn new_value<T: 'static>(path: impl Into<String>) -> Self {
        Self::new::<T>(path, ReflectKind::Value, Vec::new(), Vec::new())
    }

    pub fn new_struct<T: 'static>(path: impl Into<String>, fields: Vec<ReflectField>) -> Self {
        Self::new::<T>(path, ReflectKind::Struct, fields, Vec::new())
    }

    pub fn new_enum<T: 'static>(path: impl Into<String>, variants: Vec<ReflectVariant>) -> Self {
        Self::new::<T>(path, ReflectKind::Enum, Vec::new(), variants)
    }

    pub fn new_option<T: 'static>(path: impl Into<String>) -> Self {
        Self::new::<T>(path, ReflectKind::Option, Vec::new(), Vec::new())
    }

    pub fn new_list<T: 'static>(path: impl Into<String>) -> Self {
        Self::new::<T>(path, ReflectKind::List, Vec::new(), Vec::new())
    }

    pub fn new_array<T: 'static>(path: impl Into<String>) -> Self {
        Self::new::<T>(path, ReflectKind::Array, Vec::new(), Vec::new())
    }

    pub fn new_opaque<T: 'static>(path: impl Into<String>) -> Self {
        Self::new::<T>(path, ReflectKind::Opaque, Vec::new(), Vec::new())
    }

    fn new<T: 'static>(
        path: impl Into<String>,
        kind: ReflectKind,
        fields: Vec<ReflectField>,
        variants: Vec<ReflectVariant>,
    ) -> Self {
        Self {
            layout: type_of::<T>(),
            type_id: TypeId::of::<T>(),
            path: path.into(),
            kind,
            fields,
            variants,
        }
    }

    pub fn layout(&self) -> Type {
        self.layout
    }

    pub fn type_id(&self) -> TypeId {
        self.type_id
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn kind(&self) -> &ReflectKind {
        &self.kind
    }

    pub fn fields(&self) -> &[ReflectField] {
        &self.fields
    }

    pub fn field(&self, name: &str) -> Option<&ReflectField> {
        self.fields.iter().find(|field| field.name() == name)
    }

    pub fn variants(&self) -> &[ReflectVariant] {
        &self.variants
    }

    pub fn variant(&self, name: &str) -> Option<&ReflectVariant> {
        self.variants.iter().find(|variant| variant.name() == name)
    }
}

/// Explicit, user-owned inspector reflection registry.
#[derive(Default)]
pub struct ReflectRegistry {
    by_id: FxHashMap<TypeId, ReflectType>,
    path_to_id: FxHashMap<String, TypeId>,
}

impl ReflectRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_builtins() -> Self {
        let mut registry = Self::new();
        registry.register_builtins();
        registry
    }

    pub fn register<T: Reflect>(&mut self) -> Result<&mut Self, ReflectError> {
        let type_id = TypeId::of::<T>();
        if self.by_id.contains_key(&type_id) {
            return Ok(self);
        }

        T::reflect_dependencies(self)?;
        let ty = T::reflect_type();

        if let Some(existing_id) = self.path_to_id.get(ty.path()).copied() {
            if existing_id != type_id {
                let existing = self
                    .by_id
                    .get(&existing_id)
                    .map(|ty| ty.path().to_string())
                    .unwrap_or_else(|| "<unknown>".to_string());
                return Err(ReflectError::DuplicateTypePath {
                    path: ty.path().to_string(),
                    existing,
                    incoming: type_name::<T>().to_string(),
                });
            }
        }

        self.path_to_id.insert(ty.path().to_string(), type_id);
        self.by_id.insert(type_id, ty);
        Ok(self)
    }

    pub fn type_of<T: 'static>(&self) -> Option<&ReflectType> {
        self.by_id.get(&TypeId::of::<T>())
    }

    pub fn type_by_id(&self, type_id: TypeId) -> Option<&ReflectType> {
        self.by_id.get(&type_id)
    }

    pub fn type_by_path(&self, path: &str) -> Option<&ReflectType> {
        let type_id = self.path_to_id.get(path)?;
        self.by_id.get(type_id)
    }

    pub fn register_builtins(&mut self) {
        let _ = self
            .register::<bool>()
            .and_then(|registry| registry.register::<i8>())
            .and_then(|registry| registry.register::<i16>())
            .and_then(|registry| registry.register::<i32>())
            .and_then(|registry| registry.register::<i64>())
            .and_then(|registry| registry.register::<isize>())
            .and_then(|registry| registry.register::<u8>())
            .and_then(|registry| registry.register::<u16>())
            .and_then(|registry| registry.register::<u32>())
            .and_then(|registry| registry.register::<u64>())
            .and_then(|registry| registry.register::<usize>())
            .and_then(|registry| registry.register::<f32>())
            .and_then(|registry| registry.register::<f64>())
            .and_then(|registry| registry.register::<String>())
            .and_then(|registry| registry.register::<Vec2>())
            .and_then(|registry| registry.register::<Vec3>())
            .and_then(|registry| registry.register::<Vec4>())
            .and_then(|registry| registry.register::<Quat>())
            .and_then(|registry| registry.register::<Transform>());
    }
}

/// Helper for `"stats.hp"` nested field access.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReflectPath {
    segments: Vec<String>,
}

impl ReflectPath {
    pub fn new(path: impl AsRef<str>) -> Result<Self, ReflectError> {
        let path = path.as_ref();
        let segments = path
            .split('.')
            .filter(|segment| !segment.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>();
        if segments.is_empty() {
            return Err(ReflectError::Unsupported {
                message: "reflect path cannot be empty".to_string(),
            });
        }
        Ok(Self { segments })
    }

    pub fn read<T: Reflect>(
        &self,
        registry: &ReflectRegistry,
        root: &T,
    ) -> Result<ReflectValue, ReflectError> {
        let mut current: &dyn Any = root;
        let mut current_type =
            registry
                .type_of::<T>()
                .ok_or_else(|| ReflectError::UnknownType {
                    type_name: type_name::<T>().to_string(),
                })?;

        for (index, segment) in self.segments.iter().enumerate() {
            let field = current_type
                .field(segment)
                .ok_or_else(|| ReflectError::UnknownField {
                    type_name: current_type.path().to_string(),
                    field: segment.clone(),
                })?;

            if index == self.segments.len() - 1 {
                return field.read_any(current);
            }

            current = field.get_any(current)?;
            current_type =
                registry
                    .type_by_id(field.type_id())
                    .ok_or_else(|| ReflectError::UnknownType {
                        type_name: field.type_path().to_string(),
                    })?;
        }

        unreachable!("ReflectPath::new rejects empty paths")
    }

    pub fn write<T: Reflect>(
        &self,
        registry: &ReflectRegistry,
        root: &mut T,
        value: ReflectValue,
    ) -> Result<(), ReflectError> {
        write_path_segments(
            registry,
            TypeId::of::<T>(),
            root as &mut dyn Any,
            &self.segments,
            value,
        )
    }
}

fn write_path_segments(
    registry: &ReflectRegistry,
    current_type_id: TypeId,
    current: &mut dyn Any,
    segments: &[String],
    value: ReflectValue,
) -> Result<(), ReflectError> {
    let current_type =
        registry
            .type_by_id(current_type_id)
            .ok_or_else(|| ReflectError::UnknownType {
                type_name: "<unregistered>".to_string(),
            })?;
    let segment = &segments[0];
    let field = current_type
        .field(segment)
        .ok_or_else(|| ReflectError::UnknownField {
            type_name: current_type.path().to_string(),
            field: segment.clone(),
        })?;

    if segments.len() == 1 {
        return field.write_any(current, value);
    }

    let field_type_id = field.type_id();
    let child = field.get_any_mut(current)?;
    write_path_segments(registry, field_type_id, child, &segments[1..], value)
}

macro_rules! impl_unsigned_reflect {
    ($($ty:ty),+ $(,)?) => {
        $(
            impl Reflect for $ty {
                fn reflect_type() -> ReflectType {
                    ReflectType::new_value::<Self>(type_name::<Self>())
                }

                fn to_reflect_value(&self) -> Result<ReflectValue, ReflectError> {
                    Ok(ReflectValue::U64(*self as u64))
                }

                fn apply_reflect_value(&mut self, value: ReflectValue) -> Result<(), ReflectError> {
                    let ReflectValue::U64(value) = value else {
                        return Err(value_mismatch(stringify!($ty), &value));
                    };
                    *self = <$ty>::try_from(value).map_err(|_| ReflectError::IntegerOutOfRange {
                        target: stringify!($ty),
                        value: value.to_string(),
                    })?;
                    Ok(())
                }
            }
        )+
    };
}

macro_rules! impl_signed_reflect {
    ($($ty:ty),+ $(,)?) => {
        $(
            impl Reflect for $ty {
                fn reflect_type() -> ReflectType {
                    ReflectType::new_value::<Self>(type_name::<Self>())
                }

                fn to_reflect_value(&self) -> Result<ReflectValue, ReflectError> {
                    Ok(ReflectValue::I64(*self as i64))
                }

                fn apply_reflect_value(&mut self, value: ReflectValue) -> Result<(), ReflectError> {
                    let ReflectValue::I64(value) = value else {
                        return Err(value_mismatch(stringify!($ty), &value));
                    };
                    *self = <$ty>::try_from(value).map_err(|_| ReflectError::IntegerOutOfRange {
                        target: stringify!($ty),
                        value: value.to_string(),
                    })?;
                    Ok(())
                }
            }
        )+
    };
}

impl_signed_reflect!(i8, i16, i32, i64, isize);
impl_unsigned_reflect!(u8, u16, u32, u64, usize);

impl Reflect for bool {
    fn reflect_type() -> ReflectType {
        ReflectType::new_value::<Self>(type_name::<Self>())
    }

    fn to_reflect_value(&self) -> Result<ReflectValue, ReflectError> {
        Ok(ReflectValue::Bool(*self))
    }

    fn apply_reflect_value(&mut self, value: ReflectValue) -> Result<(), ReflectError> {
        match value {
            ReflectValue::Bool(value) => {
                *self = value;
                Ok(())
            }
            other => Err(value_mismatch("bool", &other)),
        }
    }
}

impl Reflect for f32 {
    fn reflect_type() -> ReflectType {
        ReflectType::new_value::<Self>(type_name::<Self>())
    }

    fn to_reflect_value(&self) -> Result<ReflectValue, ReflectError> {
        Ok(ReflectValue::F32(*self))
    }

    fn apply_reflect_value(&mut self, value: ReflectValue) -> Result<(), ReflectError> {
        match value {
            ReflectValue::F32(value) => {
                *self = value;
                Ok(())
            }
            ReflectValue::F64(value) => {
                *self = value as f32;
                Ok(())
            }
            other => Err(value_mismatch("f32", &other)),
        }
    }
}

impl Reflect for f64 {
    fn reflect_type() -> ReflectType {
        ReflectType::new_value::<Self>(type_name::<Self>())
    }

    fn to_reflect_value(&self) -> Result<ReflectValue, ReflectError> {
        Ok(ReflectValue::F64(*self))
    }

    fn apply_reflect_value(&mut self, value: ReflectValue) -> Result<(), ReflectError> {
        match value {
            ReflectValue::F64(value) => {
                *self = value;
                Ok(())
            }
            ReflectValue::F32(value) => {
                *self = value as f64;
                Ok(())
            }
            other => Err(value_mismatch("f64", &other)),
        }
    }
}

impl Reflect for String {
    fn reflect_type() -> ReflectType {
        ReflectType::new_value::<Self>(type_name::<Self>())
    }

    fn to_reflect_value(&self) -> Result<ReflectValue, ReflectError> {
        Ok(ReflectValue::String(self.clone()))
    }

    fn apply_reflect_value(&mut self, value: ReflectValue) -> Result<(), ReflectError> {
        match value {
            ReflectValue::String(value) => {
                *self = value;
                Ok(())
            }
            other => Err(value_mismatch("String", &other)),
        }
    }
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

impl<T: Reflect> Reflect for Option<T> {
    fn reflect_type() -> ReflectType {
        ReflectType::new_option::<Self>(type_name::<Self>())
    }

    fn reflect_dependencies(registry: &mut ReflectRegistry) -> Result<(), ReflectError> {
        registry.register::<T>()?;
        Ok(())
    }

    fn to_reflect_value(&self) -> Result<ReflectValue, ReflectError> {
        match self {
            Some(value) => Ok(ReflectValue::Option(Some(Box::new(
                value.to_reflect_value()?,
            )))),
            None => Ok(ReflectValue::Option(None)),
        }
    }

    fn apply_reflect_value(&mut self, value: ReflectValue) -> Result<(), ReflectError> {
        match value {
            ReflectValue::Option(Some(value)) => match self {
                Some(current) => current.apply_reflect_value(*value),
                None => Err(ReflectError::Unsupported {
                    message: format!(
                        "cannot construct Some<{}> through Reflect v1 without a default value",
                        type_name::<T>()
                    ),
                }),
            },
            ReflectValue::Option(None) => {
                *self = None;
                Ok(())
            }
            other => Err(value_mismatch("Option", &other)),
        }
    }
}

impl<T: Reflect> Reflect for Vec<T> {
    fn reflect_type() -> ReflectType {
        ReflectType::new_list::<Self>(type_name::<Self>())
    }

    fn reflect_dependencies(registry: &mut ReflectRegistry) -> Result<(), ReflectError> {
        registry.register::<T>()?;
        Ok(())
    }

    fn to_reflect_value(&self) -> Result<ReflectValue, ReflectError> {
        self.iter()
            .map(Reflect::to_reflect_value)
            .collect::<Result<Vec<_>, _>>()
            .map(ReflectValue::List)
    }

    fn apply_reflect_value(&mut self, value: ReflectValue) -> Result<(), ReflectError> {
        let ReflectValue::List(values) = value else {
            return Err(value_mismatch("List", &value));
        };

        if values.len() != self.len() {
            return Err(ReflectError::Unsupported {
                message: format!(
                    "Reflect v1 can edit Vec elements in place, but cannot resize Vec<{}>",
                    type_name::<T>()
                ),
            });
        }

        for (target, value) in self.iter_mut().zip(values) {
            target.apply_reflect_value(value)?;
        }
        Ok(())
    }
}

impl<T: Reflect, const N: usize> Reflect for [T; N] {
    fn reflect_type() -> ReflectType {
        ReflectType::new_array::<Self>(type_name::<Self>())
    }

    fn reflect_dependencies(registry: &mut ReflectRegistry) -> Result<(), ReflectError> {
        registry.register::<T>()?;
        Ok(())
    }

    fn to_reflect_value(&self) -> Result<ReflectValue, ReflectError> {
        self.iter()
            .map(Reflect::to_reflect_value)
            .collect::<Result<Vec<_>, _>>()
            .map(ReflectValue::Array)
    }

    fn apply_reflect_value(&mut self, value: ReflectValue) -> Result<(), ReflectError> {
        let values = match value {
            ReflectValue::Array(values) | ReflectValue::List(values) => values,
            other => return Err(value_mismatch("Array", &other)),
        };

        if values.len() != N {
            return Err(ReflectError::Unsupported {
                message: format!("expected array of length {}, got {}", N, values.len()),
            });
        }

        for (target, value) in self.iter_mut().zip(values) {
            target.apply_reflect_value(value)?;
        }
        Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reflect::ReflectEnumValue;
    use sky_engine_reflect_derive::Reflect;

    #[derive(Reflect)]
    #[reflect(name = "game.Stats")]
    struct Stats {
        #[reflect(label = "HP", min = 0, max = 100, step = 1)]
        hp: u32,
        #[reflect(readonly)]
        level: u32,
        #[reflect(skip)]
        cache: Vec<u8>,
    }

    #[derive(Reflect)]
    struct Loadout {
        stats: Stats,
        name: String,
    }

    #[derive(Reflect)]
    enum Mode {
        Idle,
        Running,
    }

    #[test]
    fn derive_named_struct_reflection_reads_attrs_and_skips_fields() {
        let mut registry = ReflectRegistry::with_builtins();
        registry.register::<Stats>().unwrap();

        let ty = registry.type_of::<Stats>().unwrap();
        assert_eq!(ty.path(), "game.Stats");
        assert_eq!(ty.kind(), &ReflectKind::Struct);
        assert_eq!(ty.fields().len(), 2);
        assert!(ty.field("cache").is_none());

        let hp = ty.field("hp").unwrap();
        assert_eq!(hp.attrs().label.as_deref(), Some("HP"));
        assert_eq!(hp.attrs().min, Some(0.0));
        assert_eq!(hp.attrs().max, Some(100.0));
        assert_eq!(hp.attrs().step, Some(1.0));
        assert!(ty.field("level").unwrap().attrs().readonly);
    }

    #[test]
    fn field_read_write_and_readonly_work() {
        let mut registry = ReflectRegistry::with_builtins();
        registry.register::<Stats>().unwrap();
        let ty = registry.type_of::<Stats>().unwrap();
        let hp = ty.field("hp").unwrap();
        let level = ty.field("level").unwrap();
        let mut stats = Stats {
            hp: 80,
            level: 3,
            cache: vec![1, 2, 3],
        };

        assert_eq!(hp.read(&stats).unwrap(), ReflectValue::U64(80));
        hp.write(&mut stats, ReflectValue::U64(90)).unwrap();
        assert_eq!(stats.hp, 90);
        assert!(matches!(
            level.write(&mut stats, ReflectValue::U64(4)),
            Err(ReflectError::ReadonlyField { .. })
        ));
        assert_eq!(stats.level, 3);
        assert_eq!(stats.cache, vec![1, 2, 3]);
    }

    #[test]
    fn reflect_path_reads_and_writes_nested_fields() {
        let mut registry = ReflectRegistry::with_builtins();
        registry.register::<Loadout>().unwrap();
        let mut loadout = Loadout {
            stats: Stats {
                hp: 40,
                level: 2,
                cache: Vec::new(),
            },
            name: "runner".to_string(),
        };

        let hp = ReflectPath::new("stats.hp").unwrap();
        assert_eq!(hp.read(&registry, &loadout).unwrap(), ReflectValue::U64(40));
        hp.write(&registry, &mut loadout, ReflectValue::U64(55))
            .unwrap();
        assert_eq!(loadout.stats.hp, 55);

        let name = ReflectPath::new("name").unwrap();
        name.write(
            &registry,
            &mut loadout,
            ReflectValue::String("scout".to_string()),
        )
        .unwrap();
        assert_eq!(loadout.name, "scout");
    }

    #[test]
    fn enum_variant_metadata_and_unit_write_work() {
        let mut registry = ReflectRegistry::with_builtins();
        registry.register::<Mode>().unwrap();
        let ty = registry.type_of::<Mode>().unwrap();
        assert_eq!(ty.kind(), &ReflectKind::Enum);
        assert!(ty.variant("Idle").is_some());

        let mut mode = Mode::Idle;
        assert_eq!(
            mode.to_reflect_value().unwrap(),
            ReflectValue::Enum(ReflectEnumValue::new(type_name::<Mode>(), "Idle"))
        );
        mode.apply_reflect_value(ReflectValue::Enum(ReflectEnumValue::new(
            type_name::<Mode>(),
            "Running",
        )))
        .unwrap();
        assert!(matches!(mode, Mode::Running));
    }

    #[test]
    fn option_vec_and_array_values_are_inspectable() {
        let mut registry = ReflectRegistry::with_builtins();
        registry.register::<Option<u32>>().unwrap();
        registry.register::<Vec<u32>>().unwrap();
        registry.register::<[u32; 2]>().unwrap();

        let opt = Some(7u32);
        assert_eq!(
            opt.to_reflect_value().unwrap(),
            ReflectValue::Option(Some(Box::new(ReflectValue::U64(7))))
        );

        let mut values = vec![1u32, 2u32];
        values
            .apply_reflect_value(ReflectValue::List(vec![
                ReflectValue::U64(3),
                ReflectValue::U64(4),
            ]))
            .unwrap();
        assert_eq!(values, vec![3, 4]);

        let mut array = [1u32, 2u32];
        array
            .apply_reflect_value(ReflectValue::Array(vec![
                ReflectValue::U64(8),
                ReflectValue::U64(9),
            ]))
            .unwrap();
        assert_eq!(array, [8, 9]);
    }

    #[test]
    fn duplicate_type_path_with_different_type_is_rejected() {
        #[derive(Reflect)]
        #[reflect(name = "game.Dupe")]
        struct A {
            value: u32,
        }

        #[derive(Reflect)]
        #[reflect(name = "game.Dupe")]
        struct B {
            value: u32,
        }

        let mut registry = ReflectRegistry::new();
        registry.register::<A>().unwrap();
        assert!(matches!(
            registry.register::<B>(),
            Err(ReflectError::DuplicateTypePath { .. })
        ));
    }
}
