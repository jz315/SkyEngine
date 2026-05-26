use serde::{Deserialize, Serialize};
use serde_json::{Map, Number, Value};

use crate::math::{Quat, Vec2, Vec3, Vec4};

/// Human-readable JSON payload for persisted components.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PersistValue(Value);

impl PersistValue {
    pub fn object() -> Self {
        Self(Value::Object(Map::new()))
    }

    pub fn array(values: impl IntoIterator<Item = impl Into<PersistValue>>) -> Self {
        Self(Value::Array(
            values.into_iter().map(|value| value.into().0).collect(),
        ))
    }

    pub fn with_field(mut self, name: impl Into<String>, value: impl Into<PersistValue>) -> Self {
        self.insert(name, value);
        self
    }

    pub fn insert(&mut self, name: impl Into<String>, value: impl Into<PersistValue>) {
        if !self.0.is_object() {
            self.0 = Value::Object(Map::new());
        }
        let Value::Object(fields) = &mut self.0 else {
            unreachable!("PersistValue was forced to an object");
        };
        fields.insert(name.into(), value.into().0);
    }

    pub fn as_json(&self) -> &Value {
        &self.0
    }
}

impl From<Value> for PersistValue {
    fn from(value: Value) -> Self {
        Self(value)
    }
}

impl From<PersistValue> for Value {
    fn from(value: PersistValue) -> Self {
        value.0
    }
}

impl From<bool> for PersistValue {
    fn from(value: bool) -> Self {
        Self(Value::Bool(value))
    }
}

macro_rules! impl_signed {
    ($($ty:ty),+ $(,)?) => {
        $(
            impl From<$ty> for PersistValue {
                fn from(value: $ty) -> Self {
                    Self(Value::Number(Number::from(value)))
                }
            }
        )+
    };
}

macro_rules! impl_unsigned {
    ($($ty:ty),+ $(,)?) => {
        $(
            impl From<$ty> for PersistValue {
                fn from(value: $ty) -> Self {
                    Self(Value::Number(Number::from(value)))
                }
            }
        )+
    };
}

impl_signed!(i8, i16, i32, i64);
impl_unsigned!(u8, u16, u32, u64);

impl From<isize> for PersistValue {
    fn from(value: isize) -> Self {
        Self(Value::Number(Number::from(value as i64)))
    }
}

impl From<usize> for PersistValue {
    fn from(value: usize) -> Self {
        Self(Value::Number(Number::from(value as u64)))
    }
}

impl From<f32> for PersistValue {
    fn from(value: f32) -> Self {
        number_value(value as f64)
    }
}

impl From<f64> for PersistValue {
    fn from(value: f64) -> Self {
        number_value(value)
    }
}

impl From<String> for PersistValue {
    fn from(value: String) -> Self {
        Self(Value::String(value))
    }
}

impl From<&str> for PersistValue {
    fn from(value: &str) -> Self {
        Self(Value::String(value.to_string()))
    }
}

impl From<Vec2> for PersistValue {
    fn from(value: Vec2) -> Self {
        Self(array_to_json(value.to_array()))
    }
}

impl From<Vec3> for PersistValue {
    fn from(value: Vec3) -> Self {
        Self(array_to_json(value.to_array()))
    }
}

impl From<Vec4> for PersistValue {
    fn from(value: Vec4) -> Self {
        Self(array_to_json(value.to_array()))
    }
}

impl From<Quat> for PersistValue {
    fn from(value: Quat) -> Self {
        Self(array_to_json(value.to_xyzw_array()))
    }
}

fn number_value(value: f64) -> PersistValue {
    PersistValue(Number::from_f64(value).map_or(Value::Null, Value::Number))
}

fn array_to_json<const N: usize>(values: [f32; N]) -> Value {
    Value::Array(
        values
            .into_iter()
            .map(|value| number_value(value as f64).0)
            .collect(),
    )
}
