use std::fmt;

/// Runtime edit/snapshot value used by inspector reflection.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(
    feature = "reflect-serde",
    derive(serde::Deserialize, serde::Serialize)
)]
#[cfg_attr(feature = "reflect-serde", serde(tag = "kind", content = "value"))]
pub enum ReflectValue {
    Unit,
    Bool(bool),
    I64(i64),
    U64(u64),
    F32(f32),
    F64(f64),
    String(String),
    Vec2([f32; 2]),
    Vec3([f32; 3]),
    Vec4([f32; 4]),
    Quat([f32; 4]),
    Struct(ReflectStructValue),
    Enum(ReflectEnumValue),
    Option(Option<Box<ReflectValue>>),
    List(Vec<ReflectValue>),
    Array(Vec<ReflectValue>),
}

impl ReflectValue {
    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::Unit => "Unit",
            Self::Bool(_) => "bool",
            Self::I64(_) => "i64",
            Self::U64(_) => "u64",
            Self::F32(_) => "f32",
            Self::F64(_) => "f64",
            Self::String(_) => "String",
            Self::Vec2(_) => "Vec2",
            Self::Vec3(_) => "Vec3",
            Self::Vec4(_) => "Vec4",
            Self::Quat(_) => "Quat",
            Self::Struct(_) => "Struct",
            Self::Enum(_) => "Enum",
            Self::Option(_) => "Option",
            Self::List(_) => "List",
            Self::Array(_) => "Array",
        }
    }
}

/// A reflected struct value with stable field order.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(
    feature = "reflect-serde",
    derive(serde::Deserialize, serde::Serialize)
)]
pub struct ReflectStructValue {
    #[cfg_attr(
        feature = "reflect-serde",
        serde(default, rename = "type", skip_serializing_if = "String::is_empty")
    )]
    type_name: String,
    #[cfg_attr(feature = "reflect-serde", serde(default))]
    fields: Vec<ReflectFieldValue>,
}

impl ReflectStructValue {
    pub fn new(type_name: impl Into<String>) -> Self {
        Self {
            type_name: type_name.into(),
            fields: Vec::new(),
        }
    }

    #[inline]
    pub fn type_name(&self) -> &str {
        &self.type_name
    }

    #[inline]
    pub fn fields(&self) -> &[ReflectFieldValue] {
        &self.fields
    }

    pub fn field(&self, name: &str) -> Option<&ReflectValue> {
        self.fields
            .iter()
            .find(|field| field.name == name)
            .map(|field| &field.value)
    }

    pub fn field_mut(&mut self, name: &str) -> Option<&mut ReflectValue> {
        self.fields
            .iter_mut()
            .find(|field| field.name == name)
            .map(|field| &mut field.value)
    }

    pub fn insert(&mut self, name: impl Into<String>, value: ReflectValue) {
        let name = name.into();
        if let Some(field) = self.fields.iter_mut().find(|field| field.name == name) {
            field.value = value;
        } else {
            self.fields.push(ReflectFieldValue { name, value });
        }
    }

    pub fn with_field(mut self, name: impl Into<String>, value: ReflectValue) -> Self {
        self.insert(name, value);
        self
    }
}

/// One named field inside [`ReflectStructValue`].
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(
    feature = "reflect-serde",
    derive(serde::Deserialize, serde::Serialize)
)]
pub struct ReflectFieldValue {
    name: String,
    value: ReflectValue,
}

impl ReflectFieldValue {
    #[inline]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[inline]
    pub fn value(&self) -> &ReflectValue {
        &self.value
    }

    #[inline]
    pub fn into_parts(self) -> (String, ReflectValue) {
        (self.name, self.value)
    }
}

/// Current enum variant snapshot. V1 writes unit variants; richer variant
/// payload editing can layer on top of this shape later.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(
    feature = "reflect-serde",
    derive(serde::Deserialize, serde::Serialize)
)]
pub struct ReflectEnumValue {
    #[cfg_attr(
        feature = "reflect-serde",
        serde(default, rename = "type", skip_serializing_if = "String::is_empty")
    )]
    type_name: String,
    variant: String,
    #[cfg_attr(feature = "reflect-serde", serde(default))]
    fields: Vec<ReflectFieldValue>,
}

impl ReflectEnumValue {
    pub fn new(type_name: impl Into<String>, variant: impl Into<String>) -> Self {
        Self {
            type_name: type_name.into(),
            variant: variant.into(),
            fields: Vec::new(),
        }
    }

    #[inline]
    pub fn type_name(&self) -> &str {
        &self.type_name
    }

    #[inline]
    pub fn variant(&self) -> &str {
        &self.variant
    }

    #[inline]
    pub fn fields(&self) -> &[ReflectFieldValue] {
        &self.fields
    }

    pub fn with_field(mut self, name: impl Into<String>, value: ReflectValue) -> Self {
        self.fields.push(ReflectFieldValue {
            name: name.into(),
            value,
        });
        self
    }
}

/// Error produced by inspector reflection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReflectError {
    OwnerTypeMismatch {
        expected: String,
        actual: String,
    },
    ValueTypeMismatch {
        expected: &'static str,
        actual: &'static str,
    },
    IntegerOutOfRange {
        target: &'static str,
        value: String,
    },
    StructTypeMismatch {
        expected: String,
        actual: String,
    },
    EnumTypeMismatch {
        expected: String,
        actual: String,
    },
    UnknownField {
        type_name: String,
        field: String,
    },
    UnknownType {
        type_name: String,
    },
    DuplicateTypePath {
        path: String,
        existing: String,
        incoming: String,
    },
    ReadonlyField {
        field: String,
    },
    TypeReadUnsupported {
        type_name: String,
    },
    TypeWriteUnsupported {
        type_name: String,
    },
    ReadUnsupported {
        field: String,
    },
    WriteUnsupported {
        field: String,
    },
    Unsupported {
        message: String,
    },
}

impl fmt::Display for ReflectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OwnerTypeMismatch { expected, actual } => {
                write!(f, "field belongs to '{expected}', not '{actual}'")
            }
            Self::ValueTypeMismatch { expected, actual } => {
                write!(f, "expected value kind '{expected}', got '{actual}'")
            }
            Self::IntegerOutOfRange { target, value } => {
                write!(f, "integer value {value} is out of range for {target}")
            }
            Self::StructTypeMismatch { expected, actual } => {
                write!(f, "expected struct value for '{expected}', got '{actual}'")
            }
            Self::EnumTypeMismatch { expected, actual } => {
                write!(f, "expected enum value for '{expected}', got '{actual}'")
            }
            Self::UnknownField { type_name, field } => {
                write!(f, "type '{type_name}' has no reflected field '{field}'")
            }
            Self::UnknownType { type_name } => {
                write!(f, "type '{type_name}' is not registered for reflection")
            }
            Self::DuplicateTypePath {
                path,
                existing,
                incoming,
            } => {
                write!(
                    f,
                    "reflect type path '{path}' is already used by '{existing}', cannot register '{incoming}'"
                )
            }
            Self::ReadonlyField { field } => write!(f, "field '{field}' is readonly"),
            Self::TypeReadUnsupported { type_name } => {
                write!(f, "type '{type_name}' cannot be read as ReflectValue")
            }
            Self::TypeWriteUnsupported { type_name } => {
                write!(f, "type '{type_name}' cannot be written from ReflectValue")
            }
            Self::ReadUnsupported { field } => write!(f, "field '{field}' cannot be read"),
            Self::WriteUnsupported { field } => write!(f, "field '{field}' cannot be written"),
            Self::Unsupported { message } => f.write_str(message),
        }
    }
}

impl std::error::Error for ReflectError {}
