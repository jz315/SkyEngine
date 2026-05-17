use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::render::Color;

/// Format-neutral custom property value.
#[derive(Clone, Debug)]
pub enum PropertyValue {
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Color(Color),
    File(PathBuf),
    Object(u64),
}

impl PartialEq for PropertyValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Bool(a), Self::Bool(b)) => a == b,
            (Self::Int(a), Self::Int(b)) => a == b,
            (Self::Float(a), Self::Float(b)) => a.to_bits() == b.to_bits(),
            (Self::String(a), Self::String(b)) => a == b,
            (Self::Color(a), Self::Color(b)) => {
                a.r.to_bits() == b.r.to_bits()
                    && a.g.to_bits() == b.g.to_bits()
                    && a.b.to_bits() == b.b.to_bits()
                    && a.a.to_bits() == b.a.to_bits()
            }
            (Self::File(a), Self::File(b)) => a == b,
            (Self::Object(a), Self::Object(b)) => a == b,
            _ => false,
        }
    }
}

impl From<bool> for PropertyValue {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl From<i32> for PropertyValue {
    fn from(value: i32) -> Self {
        Self::Int(value as i64)
    }
}

impl From<i64> for PropertyValue {
    fn from(value: i64) -> Self {
        Self::Int(value)
    }
}

impl From<f32> for PropertyValue {
    fn from(value: f32) -> Self {
        Self::Float(value as f64)
    }
}

impl From<f64> for PropertyValue {
    fn from(value: f64) -> Self {
        Self::Float(value)
    }
}

impl From<String> for PropertyValue {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

impl From<&str> for PropertyValue {
    fn from(value: &str) -> Self {
        Self::String(value.to_string())
    }
}

/// Deterministic custom property storage.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PropertyBag {
    values: BTreeMap<String, PropertyValue>,
}

impl PropertyBag {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(
        &mut self,
        key: impl Into<String>,
        value: impl Into<PropertyValue>,
    ) -> Option<PropertyValue> {
        self.values.insert(key.into(), value.into())
    }

    pub fn get(&self, key: &str) -> Option<&PropertyValue> {
        self.values.get(key)
    }

    pub fn remove(&mut self, key: &str) -> Option<PropertyValue> {
        self.values.remove(key)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &PropertyValue)> {
        self.values.iter().map(|(key, value)| (key.as_str(), value))
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}
