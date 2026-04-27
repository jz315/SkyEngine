use std::fmt;

use serde::de::{MapAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

use crate::math::Transform;

use super::{SceneComponents, SceneError, SceneValue, TRANSFORM_COMPONENT_TYPE};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
struct SceneTransformData {
    position: [f32; 3],
    #[serde(default)]
    rotation_z: f32,
    #[serde(default = "default_scale")]
    scale: [f32; 3],
}

impl From<Transform> for SceneTransformData {
    fn from(transform: Transform) -> Self {
        Self {
            position: transform.position.to_array(),
            rotation_z: transform.rotation_z(),
            scale: transform.scale.to_array(),
        }
    }
}

impl From<SceneTransformData> for Transform {
    fn from(data: SceneTransformData) -> Self {
        Transform::from_xyz(data.position[0], data.position[1], data.position[2])
            .with_rotation(data.rotation_z)
            .with_scale3(data.scale[0], data.scale[1], data.scale[2])
    }
}

pub(crate) fn transform_to_scene_value(transform: Transform) -> SceneValue {
    SceneValue::from(
        serde_json::to_value(SceneTransformData::from(transform))
            .expect("scene transform data should always serialize"),
    )
}

pub(crate) fn scene_value_to_transform(value: &SceneValue) -> Result<Transform, SceneError> {
    serde_json::from_value::<SceneTransformData>(value.as_json().clone())
        .map(Transform::from)
        .map_err(|error| SceneError::ComponentSerde {
            type_name: TRANSFORM_COMPONENT_TYPE.to_string(),
            error: error.to_string(),
        })
}

impl Serialize for SceneComponents {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(self.len()))?;
        for (type_name, value) in self.iter() {
            map.serialize_entry(type_name, value.as_json())?;
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for SceneComponents {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(SceneComponentsVisitor)
    }
}

struct SceneComponentsVisitor;

impl<'de> Visitor<'de> for SceneComponentsVisitor {
    type Value = SceneComponents;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("scene components object map")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut components = SceneComponents::new();
        while let Some((type_name, value)) = map.next_entry::<String, Value>()? {
            if components.contains(&type_name) {
                components.push_raw(type_name, SceneValue::from(value));
            } else {
                components.insert(type_name, SceneValue::from(value));
            }
        }
        Ok(components)
    }
}

fn default_scale() -> [f32; 3] {
    [1.0, 1.0, 1.0]
}
