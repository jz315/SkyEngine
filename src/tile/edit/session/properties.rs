use super::super::super::palette::PropertyValue;
use super::super::summary::{PropertyChange, PropertyTarget};
use super::TileMapEditSession;

impl TileMapEditSession<'_> {
    pub fn set_property(
        &mut self,
        target: PropertyTarget,
        key: &str,
        value: impl Into<PropertyValue>,
    ) {
        let value = value.into();
        let mut layer_to_mark = None;
        let old = match target {
            PropertyTarget::Scene => self.scene.properties.get(key).cloned(),
            PropertyTarget::Layer(layer) => {
                let layer_ref = self.scene.layer(layer);
                layer_ref.and_then(|layer_ref| layer_ref.properties.get(key).cloned())
            }
            PropertyTarget::Object(object_id) => {
                let object = self.scene.objects.get(object_id);
                object.and_then(|object| object.properties.get(key).cloned())
            }
        };
        if old.as_ref() == Some(&value) {
            return;
        }
        match target {
            PropertyTarget::Scene => {
                let _ = self.scene.properties.insert(key, value.clone());
            }
            PropertyTarget::Layer(layer) => {
                let Some(layer_ref) = self.scene.layer_mut(layer) else {
                    return;
                };
                let _ = layer_ref.properties.insert(key, value.clone());
                layer_to_mark = Some(layer);
            }
            PropertyTarget::Object(object_id) => {
                let Some(object) = self.scene.objects.get_mut(object_id) else {
                    return;
                };
                let _ = object.properties.insert(key, value.clone());
                layer_to_mark = Some(object.layer);
            }
        }
        if let Some(layer) = layer_to_mark {
            self.summary.mark_layer(layer);
        }
        self.summary.property_changes.push(PropertyChange {
            target,
            key: key.to_string(),
            old,
            new: Some(value),
        });
    }

    pub fn remove_property(&mut self, target: PropertyTarget, key: &str) {
        let mut layer_to_mark = None;
        let old = match &target {
            PropertyTarget::Scene => self.scene.properties.get(key).cloned(),
            PropertyTarget::Layer(layer) => self
                .scene
                .layer(*layer)
                .and_then(|layer_ref| layer_ref.properties.get(key).cloned()),
            PropertyTarget::Object(object_id) => self
                .scene
                .objects
                .get(*object_id)
                .and_then(|object| object.properties.get(key).cloned()),
        };
        if old.is_none() {
            return;
        }
        match target {
            PropertyTarget::Scene => {
                let _ = self.scene.properties.remove(key);
            }
            PropertyTarget::Layer(layer) => {
                let Some(layer_ref) = self.scene.layer_mut(layer) else {
                    return;
                };
                let _ = layer_ref.properties.remove(key);
                layer_to_mark = Some(layer);
            }
            PropertyTarget::Object(object_id) => {
                let Some(object) = self.scene.objects.get_mut(object_id) else {
                    return;
                };
                let _ = object.properties.remove(key);
                layer_to_mark = Some(object.layer);
            }
        }
        if let Some(layer) = layer_to_mark {
            self.summary.mark_layer(layer);
        }
        self.summary.property_changes.push(PropertyChange {
            target,
            key: key.to_string(),
            old,
            new: None,
        });
    }
}
