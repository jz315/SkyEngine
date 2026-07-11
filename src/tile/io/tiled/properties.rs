use crate::tile::{LayerRole, PropertyBag, PropertyValue};

use super::import::{TiledProperty, TiledPropertyValue};

pub(super) fn property_bag_from_tiled(properties: &[TiledProperty]) -> PropertyBag {
    let mut bag = PropertyBag::new();
    for property in properties {
        bag.insert(
            property.name.clone(),
            property_value_from_tiled(&property.value),
        );
    }
    bag
}

fn property_value_from_tiled(value: &TiledPropertyValue) -> PropertyValue {
    match value {
        TiledPropertyValue::Bool(value) => PropertyValue::Bool(*value),
        TiledPropertyValue::Int(value) => PropertyValue::Int(*value as i64),
        TiledPropertyValue::Float(value) => PropertyValue::Float(*value as f64),
        TiledPropertyValue::String(value) => PropertyValue::String(value.clone()),
        TiledPropertyValue::Color(color) => PropertyValue::Color(*color),
        TiledPropertyValue::File(path) => PropertyValue::File(path.clone()),
        TiledPropertyValue::Object(id) => PropertyValue::Object(*id as u64),
    }
}

pub(super) fn infer_layer_role(name: &str) -> LayerRole {
    match name.trim().to_ascii_lowercase().as_str() {
        "ground" | "platforms" | "terrain" => LayerRole::Ground,
        "detail" | "details" => LayerRole::Detail,
        "props" | "objects" => LayerRole::Props,
        "walls" => LayerRole::Walls,
        "upper" | "foreground" => LayerRole::Upper,
        "collision" | "collisions" => LayerRole::Collision,
        "gameplay" => LayerRole::Gameplay,
        "preview" => LayerRole::Preview,
        _ => LayerRole::Custom(name.to_string()),
    }
}
