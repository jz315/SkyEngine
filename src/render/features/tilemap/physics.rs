use std::fmt;

use crate::ecs::{EntityId, World};
use crate::math::Transform;
use crate::physics::{Collider2D, RigidBody2D};
use crate::render::features::tilemap::TilemapOrientation;

use super::{TiledImport, TiledObjectShape, TiledProperty, TiledPropertyValue};

/// Options for spawning physics colliders from a Tiled import.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TiledPhysicsOptions {
    pub spawn_tile_colliders: bool,
    pub spawn_object_colliders: bool,
    pub include_invisible_layers: bool,
}

impl Default for TiledPhysicsOptions {
    fn default() -> Self {
        Self {
            spawn_tile_colliders: true,
            spawn_object_colliders: true,
            include_invisible_layers: true,
        }
    }
}

/// Entities spawned for one Tiled physics import.
#[derive(Debug)]
pub struct TiledPhysicsInstance {
    pub entities: Vec<EntityId>,
}

impl TiledPhysicsInstance {
    pub fn spawn(
        world: &mut World,
        import: &TiledImport,
        origin: [f32; 2],
        options: TiledPhysicsOptions,
    ) -> Result<Self, TiledPhysicsError> {
        let mut specs = Vec::new();
        if options.spawn_tile_colliders {
            collect_tile_colliders(import, origin, options, &mut specs)?;
        }
        if options.spawn_object_colliders {
            collect_object_colliders(import, origin, options, &mut specs)?;
        }

        let entities = specs
            .into_iter()
            .map(|spec| {
                world.spawn((
                    Transform::from_xy(spec.center[0], spec.center[1]),
                    RigidBody2D::static_body(),
                    Collider2D::rectangle(spec.size[0], spec.size[1]).sensor(spec.sensor),
                ))
            })
            .collect();
        Ok(Self { entities })
    }

    pub fn despawn(self, world: &mut World) {
        for entity in self.entities {
            let _ = world.despawn(entity);
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TiledPhysicsError {
    UnsupportedTileOrientation {
        orientation: TilemapOrientation,
    },
    UnsupportedObjectShape {
        layer: String,
        object_id: u32,
        shape: &'static str,
    },
}

impl fmt::Display for TiledPhysicsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedTileOrientation { orientation } => write!(
                f,
                "tile colliders are only supported for orthogonal maps in v1; got {orientation:?}"
            ),
            Self::UnsupportedObjectShape {
                layer,
                object_id,
                shape,
            } => write!(
                f,
                "object {object_id} in layer '{layer}' uses unsupported physics shape {shape}"
            ),
        }
    }
}

impl std::error::Error for TiledPhysicsError {}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ColliderSpec {
    center: [f32; 2],
    size: [f32; 2],
    sensor: bool,
}

fn collect_tile_colliders(
    import: &TiledImport,
    origin: [f32; 2],
    options: TiledPhysicsOptions,
    specs: &mut Vec<ColliderSpec>,
) -> Result<(), TiledPhysicsError> {
    let has_tile_collision = import.layers.iter().any(|layer| {
        (options.include_invisible_layers || layer.visible)
            && (property_bool(&layer.properties, "solid")
                || property_bool(&layer.properties, "trigger")
                || layer_has_collision_tiles(import, layer.source_layer))
    });
    if has_tile_collision && import.orientation != TilemapOrientation::Orthogonal {
        return Err(TiledPhysicsError::UnsupportedTileOrientation {
            orientation: import.orientation,
        });
    }

    for layer in &import.layers {
        if !options.include_invisible_layers && !layer.visible {
            continue;
        }
        let layer_solid = property_bool(&layer.properties, "solid");
        let layer_trigger = property_bool(&layer.properties, "trigger");
        let width = import.map.width() as usize;
        let height = import.map.height() as usize;
        let mut solid = vec![false; width * height];
        let mut trigger = vec![false; width * height];

        for y in 0..height {
            for x in 0..width {
                let Some(tile) = import.map.tile(layer.storage_layer, x as u32, y as u32) else {
                    continue;
                };
                if tile.is_empty() {
                    continue;
                }
                let tile_props = import
                    .tilesets
                    .get(layer.tileset_index)
                    .and_then(|tileset| tileset.tile_properties.get(tile.id.0 as usize))
                    .map(Vec::as_slice)
                    .unwrap_or(&[]);
                let is_trigger = layer_trigger || property_bool(tile_props, "trigger");
                let is_solid = is_trigger || layer_solid || property_bool(tile_props, "solid");
                let index = y * width + x;
                if is_trigger {
                    trigger[index] = true;
                } else if is_solid {
                    solid[index] = true;
                }
            }
        }

        merge_grid(
            &solid,
            width,
            height,
            import.tile_size,
            [origin[0] + layer.offset[0], origin[1] + layer.offset[1]],
            false,
            specs,
        );
        merge_grid(
            &trigger,
            width,
            height,
            import.tile_size,
            [origin[0] + layer.offset[0], origin[1] + layer.offset[1]],
            true,
            specs,
        );
    }

    Ok(())
}

fn collect_object_colliders(
    import: &TiledImport,
    origin: [f32; 2],
    options: TiledPhysicsOptions,
    specs: &mut Vec<ColliderSpec>,
) -> Result<(), TiledPhysicsError> {
    for layer in &import.object_layers {
        if !options.include_invisible_layers && !layer.visible {
            continue;
        }
        let layer_solid = property_bool(&layer.properties, "solid");
        let layer_trigger = property_bool(&layer.properties, "trigger");
        for object in &layer.objects {
            let is_trigger = layer_trigger || property_bool(&object.properties, "trigger");
            let is_solid = is_trigger || layer_solid || property_bool(&object.properties, "solid");
            if !is_solid {
                continue;
            }

            if !matches!(object.shape, TiledObjectShape::Rectangle) {
                return Err(TiledPhysicsError::UnsupportedObjectShape {
                    layer: layer.name.clone(),
                    object_id: object.id,
                    shape: object_shape_name(&object.shape),
                });
            }

            let center = object.center();
            specs.push(ColliderSpec {
                center: [
                    origin[0] + layer.offset[0] + center[0],
                    origin[1] + layer.offset[1] + center[1],
                ],
                size: [object.size[0], object.size[1]],
                sensor: is_trigger,
            });
        }
    }
    Ok(())
}

fn layer_has_collision_tiles(import: &TiledImport, source_layer: usize) -> bool {
    let Some(layer) = import
        .layers
        .iter()
        .find(|layer| layer.source_layer == source_layer)
    else {
        return false;
    };
    let Some(tileset) = import.tilesets.get(layer.tileset_index) else {
        return false;
    };
    tileset.tile_properties.iter().any(|properties| {
        property_bool(properties, "solid") || property_bool(properties, "trigger")
    })
}

fn merge_grid(
    grid: &[bool],
    width: usize,
    height: usize,
    tile_size: [u32; 2],
    origin: [f32; 2],
    sensor: bool,
    specs: &mut Vec<ColliderSpec>,
) {
    let mut visited = vec![false; grid.len()];
    for y in 0..height {
        for x in 0..width {
            let index = y * width + x;
            if visited[index] || !grid[index] {
                continue;
            }

            let mut rect_w = 1usize;
            while x + rect_w < width {
                let next = y * width + x + rect_w;
                if visited[next] || !grid[next] {
                    break;
                }
                rect_w += 1;
            }

            let mut rect_h = 1usize;
            'rows: while y + rect_h < height {
                for xx in x..x + rect_w {
                    let next = (y + rect_h) * width + xx;
                    if visited[next] || !grid[next] {
                        break 'rows;
                    }
                }
                rect_h += 1;
            }

            for yy in y..y + rect_h {
                for xx in x..x + rect_w {
                    visited[yy * width + xx] = true;
                }
            }

            let tile_w = tile_size[0] as f32;
            let tile_h = tile_size[1] as f32;
            let size = [rect_w as f32 * tile_w, rect_h as f32 * tile_h];
            let center = [
                origin[0] + x as f32 * tile_w + size[0] * 0.5,
                origin[1] + y as f32 * tile_h + size[1] * 0.5,
            ];
            specs.push(ColliderSpec {
                center,
                size,
                sensor,
            });
        }
    }
}

fn property_bool(properties: &[TiledProperty], name: &str) -> bool {
    properties
        .iter()
        .find(|property| property.name == name)
        .is_some_and(|property| match &property.value {
            TiledPropertyValue::Bool(value) => *value,
            TiledPropertyValue::Int(value) => *value != 0,
            TiledPropertyValue::Float(value) => *value != 0.0,
            TiledPropertyValue::String(value) => matches!(
                value.as_str(),
                "true" | "True" | "TRUE" | "1" | "yes" | "Yes" | "YES"
            ),
            _ => false,
        })
}

fn object_shape_name(shape: &TiledObjectShape) -> &'static str {
    match shape {
        TiledObjectShape::Rectangle => "rectangle",
        TiledObjectShape::Point => "point",
        TiledObjectShape::Ellipse => "ellipse",
        TiledObjectShape::Polygon(_) => "polygon",
        TiledObjectShape::Polyline(_) => "polyline",
        TiledObjectShape::Tile { .. } => "tile",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset::{AssetId, Handle, TextureAsset};
    use crate::physics::{PhysicsConfig2D, PhysicsPlugin, PhysicsWorld2D};
    use crate::plugin::Plugin;
    use crate::render::features::tilemap::{TiledMapInstance, TiledSpawnOptions};

    #[test]
    fn spawns_merged_solid_and_trigger_tile_colliders() {
        let import = TiledImport::from_tmx_str(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<map version="1.10" orientation="orthogonal" renderorder="right-down" width="4" height="3" tilewidth="16" tileheight="16">
 <tileset firstgid="1" name="test" tilewidth="16" tileheight="16" tilecount="3" columns="3">
  <image source="dummy.png" width="48" height="16"/>
  <tile id="1"><properties><property name="solid" type="bool" value="true"/></properties></tile>
  <tile id="2"><properties><property name="trigger" type="bool" value="true"/></properties></tile>
 </tileset>
 <layer id="1" name="Ground" width="4" height="3">
  <data encoding="csv">
2,2,0,3,
2,2,0,3,
0,0,0,0
  </data>
 </layer>
</map>"#,
            ".",
        )
        .unwrap();
        let mut world = World::new();
        PhysicsPlugin::new(PhysicsConfig2D::default())
            .install(&mut world)
            .unwrap();
        let instance =
            TiledPhysicsInstance::spawn(&mut world, &import, [10.0, 20.0], Default::default())
                .unwrap();

        assert_eq!(instance.entities.len(), 2);
        world.tick_with_delta(1.0 / 60.0).unwrap();
        assert_eq!(
            world
                .get_resource::<PhysicsWorld2D>()
                .unwrap()
                .collider_count(),
            2
        );
        assert!(instance.entities.iter().any(|entity| {
            let transform = world.get::<Transform>(*entity).unwrap();
            let collider = world.get::<Collider2D>(*entity).unwrap();
            transform.position.x() == 26.0 && transform.position.y() == 52.0 && !collider.sensor
        }));
        assert!(instance.entities.iter().any(|entity| {
            let transform = world.get::<Transform>(*entity).unwrap();
            let collider = world.get::<Collider2D>(*entity).unwrap();
            transform.position.x() == 66.0 && transform.position.y() == 52.0 && collider.sensor
        }));
    }

    #[test]
    fn spawns_rectangle_object_colliders() {
        let import = TiledImport::from_tmx_str(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<map version="1.10" orientation="orthogonal" renderorder="right-down" width="2" height="2" tilewidth="16" tileheight="16">
 <tileset firstgid="1" name="test" tilewidth="16" tileheight="16" tilecount="1" columns="1">
  <image source="dummy.png" width="16" height="16"/>
 </tileset>
 <layer id="1" name="Ground" width="2" height="2"><data encoding="csv">0,0,0,0</data></layer>
 <objectgroup id="2" name="Collision" offsetx="4" offsety="6">
  <object id="7" x="10" y="20" width="30" height="40">
   <properties><property name="trigger" type="bool" value="true"/></properties>
  </object>
 </objectgroup>
</map>"#,
            ".",
        )
        .unwrap();
        let mut world = World::new();
        let instance =
            TiledPhysicsInstance::spawn(&mut world, &import, [1.0, 2.0], Default::default())
                .unwrap();
        assert_eq!(instance.entities.len(), 1);
        let entity = instance.entities[0];
        let transform = world.get::<Transform>(entity).unwrap();
        let collider = world.get::<Collider2D>(entity).unwrap();
        assert_eq!(
            [transform.position.x(), transform.position.y()],
            [30.0, 40.0]
        );
        assert!(collider.sensor);
    }

    #[test]
    fn centered_render_origin_aligns_tile_physics_colliders() {
        let import = TiledImport::from_tmx_str(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<map version="1.10" orientation="orthogonal" renderorder="right-down" width="2" height="2" tilewidth="16" tileheight="16">
 <tileset firstgid="1" name="test" tilewidth="16" tileheight="16" tilecount="2" columns="2">
  <image source="dummy.png" width="32" height="16"/>
  <tile id="1"><properties><property name="solid" type="bool" value="true"/></properties></tile>
 </tileset>
 <layer id="1" name="Ground" width="2" height="2">
  <data encoding="csv">
1,1,
2,1
  </data>
 </layer>
</map>"#,
            ".",
        )
        .unwrap();
        let mut world = World::new();
        let texture = Handle::<TextureAsset>::new(AssetId::new());
        let render = TiledMapInstance::spawn_import_with_textures(
            &mut world,
            &import,
            vec![texture],
            TiledSpawnOptions::centered().with_parallax(false),
        )
        .unwrap();
        let physics =
            TiledPhysicsInstance::spawn(&mut world, &import, render.origin(), Default::default())
                .unwrap();

        assert_eq!(render.origin(), [-16.0, -16.0]);
        assert_eq!(physics.entities.len(), 1);
        let transform = world.get::<Transform>(physics.entities[0]).unwrap();
        assert_eq!(
            [transform.position.x(), transform.position.y()],
            [-8.0, -8.0]
        );
    }

    #[test]
    fn tiled_physics_instance_despawn_cleans_internal_handles() {
        let import = TiledImport::from_tmx_str(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<map version="1.10" orientation="orthogonal" renderorder="right-down" width="1" height="1" tilewidth="16" tileheight="16">
 <tileset firstgid="1" name="test" tilewidth="16" tileheight="16" tilecount="1" columns="1">
  <image source="dummy.png" width="16" height="16"/>
  <tile id="0"><properties><property name="solid" type="bool" value="true"/></properties></tile>
 </tileset>
 <layer id="1" name="Ground" width="1" height="1"><data encoding="csv">1</data></layer>
</map>"#,
            ".",
        )
        .unwrap();
        let mut world = World::new();
        PhysicsPlugin::new(PhysicsConfig2D::default())
            .install(&mut world)
            .unwrap();
        let instance =
            TiledPhysicsInstance::spawn(&mut world, &import, [0.0, 0.0], Default::default())
                .unwrap();
        world.tick_with_delta(1.0 / 60.0).unwrap();
        assert_eq!(
            world
                .get_resource::<PhysicsWorld2D>()
                .unwrap()
                .collider_count(),
            1
        );

        instance.despawn(&mut world);
        world.tick_with_delta(1.0 / 60.0).unwrap();

        let physics = world.get_resource::<PhysicsWorld2D>().unwrap();
        assert_eq!(physics.body_count(), 0);
        assert_eq!(physics.collider_count(), 0);
    }

    #[test]
    fn demo_map_asset_spawns_solid_and_trigger_colliders() {
        let import = TiledImport::from_tmx_file("examples/assets/tiled/physics_topdown.tmx")
            .expect("demo physics map should import");
        let mut world = World::new();
        let instance =
            TiledPhysicsInstance::spawn(&mut world, &import, [0.0, 0.0], Default::default())
                .expect("demo physics colliders should spawn");

        assert!(instance.entities.len() >= 2);
        assert!(instance.entities.iter().any(|entity| {
            world
                .get::<Collider2D>(*entity)
                .is_some_and(|collider| !collider.sensor)
        }));
        assert!(instance.entities.iter().any(|entity| {
            world
                .get::<Collider2D>(*entity)
                .is_some_and(|collider| collider.sensor)
        }));
    }

    #[test]
    fn non_orthogonal_tile_colliders_are_rejected() {
        let import = TiledImport::from_tmx_str(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<map version="1.10" orientation="isometric" renderorder="right-down" width="1" height="1" tilewidth="16" tileheight="16">
 <tileset firstgid="1" name="test" tilewidth="16" tileheight="16" tilecount="1" columns="1">
  <image source="dummy.png" width="16" height="16"/>
  <tile id="0"><properties><property name="solid" type="bool" value="true"/></properties></tile>
 </tileset>
 <layer id="1" name="Ground" width="1" height="1"><data encoding="csv">1</data></layer>
</map>"#,
            ".",
        )
        .unwrap();
        let mut world = World::new();
        let err = TiledPhysicsInstance::spawn(&mut world, &import, [0.0, 0.0], Default::default())
            .unwrap_err();
        assert!(matches!(
            err,
            TiledPhysicsError::UnsupportedTileOrientation { .. }
        ));
    }
}
