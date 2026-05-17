use crate::ecs::World;
use crate::render::TilemapStorage;

use super::super::document::TileMapDocument;
use super::super::edit::TileMapEditSummary;
use super::super::grid::{CellCoord, CellRect};
use super::super::layer::LayerId;
use super::super::palette::TilePaletteStore;
use super::super::scene::TileMap;
use super::super::sync::TileMapRenderSync;
use super::runtime::{spawn_render_entities, TileMapInstance, TileMapInstanceError};

impl TileMapInstance {
    pub fn refresh(
        &mut self,
        world: &mut World,
        scene: &TileMap,
    ) -> Result<(), TileMapInstanceError> {
        let palettes = self.palettes.clone();
        self.refresh_with_palettes(world, scene, palettes)
    }

    pub fn refresh_with_palettes(
        &mut self,
        world: &mut World,
        scene: &TileMap,
        palettes: TilePaletteStore,
    ) -> Result<(), TileMapInstanceError> {
        for entity in self.entities.drain(..) {
            let _ = world.despawn(entity);
        }
        let storage = world
            .get_resource_mut::<TilemapStorage>()
            .ok_or(TileMapInstanceError::MissingTilemapStorage)?;
        let _ = storage.remove(self.scene);
        let (map, layers) = TileMapRenderSync::insert(scene, &palettes, storage)?;
        self.entities = spawn_render_entities(world, scene, &layers, self.origin);
        self.scene = map;
        self.layers = layers;
        self.palettes = palettes;
        Ok(())
    }

    pub fn refresh_document(
        &mut self,
        world: &mut World,
        document: &TileMapDocument,
    ) -> Result<(), TileMapInstanceError> {
        self.refresh_with_palettes(world, &document.scene, document.palettes.clone())
    }

    pub fn refresh_changed_layers(
        &mut self,
        world: &mut World,
        scene: &TileMap,
        changed_layers: impl IntoIterator<Item = LayerId>,
    ) -> Result<(), TileMapInstanceError> {
        let changed_layers = normalized_layers(changed_layers);
        if changed_layers.is_empty() {
            return Ok(());
        }

        if !self.layer_splits_match(scene, &changed_layers) {
            return self.refresh(world, scene);
        }

        let storage = world
            .get_resource_mut::<TilemapStorage>()
            .ok_or(TileMapInstanceError::MissingTilemapStorage)?;
        let tilemap = storage
            .get_mut(self.scene)
            .ok_or(TileMapInstanceError::MissingTilemapStorage)?;
        for layer_id in changed_layers {
            for layer in self
                .layers
                .iter()
                .filter(|layer| layer.source_layer == layer_id)
            {
                tilemap.clear_layer(layer.storage_layer);
                TileMapRenderSync::write_layer_to_tilemap(
                    scene,
                    tilemap,
                    layer.source_layer,
                    layer.palette,
                    layer.storage_layer,
                );
            }
        }
        Ok(())
    }

    pub fn refresh_document_changed_layers(
        &mut self,
        world: &mut World,
        document: &TileMapDocument,
        changed_layers: impl IntoIterator<Item = LayerId>,
    ) -> Result<(), TileMapInstanceError> {
        let changed_layers = normalized_layers(changed_layers);
        if changed_layers.is_empty() {
            return Ok(());
        }

        if !self.layer_splits_match(&document.scene, &changed_layers) {
            return self.refresh_with_palettes(world, &document.scene, document.palettes.clone());
        }

        self.refresh_changed_layers(world, &document.scene, changed_layers)
    }

    pub fn refresh_edit_summary(
        &mut self,
        world: &mut World,
        scene: &TileMap,
        summary: &TileMapEditSummary,
    ) -> Result<(), TileMapInstanceError> {
        self.refresh_edit_summary_inner(world, scene, None, summary)
    }

    pub fn refresh_document_edit_summary(
        &mut self,
        world: &mut World,
        document: &TileMapDocument,
        summary: &TileMapEditSummary,
    ) -> Result<(), TileMapInstanceError> {
        self.refresh_edit_summary_inner(
            world,
            &document.scene,
            Some(document.palettes.clone()),
            summary,
        )
    }

    fn refresh_edit_summary_inner(
        &mut self,
        world: &mut World,
        scene: &TileMap,
        rebuild_palettes: Option<TilePaletteStore>,
        summary: &TileMapEditSummary,
    ) -> Result<(), TileMapInstanceError> {
        let changed_layers = normalized_layers(summary.changed_layers.iter().copied());
        if changed_layers.is_empty() {
            return Ok(());
        }

        if !self.layer_splits_match(scene, &changed_layers) {
            return match rebuild_palettes {
                Some(palettes) => self.refresh_with_palettes(world, scene, palettes),
                None => self.refresh(world, scene),
            };
        }

        if !summary.object_changes.is_empty() {
            return self.refresh_changed_layers(world, scene, changed_layers);
        }

        let storage = world
            .get_resource_mut::<TilemapStorage>()
            .ok_or(TileMapInstanceError::MissingTilemapStorage)?;
        let tilemap = storage
            .get_mut(self.scene)
            .ok_or(TileMapInstanceError::MissingTilemapStorage)?;

        for dirty in &summary.dirty_cells {
            self.refresh_cell(scene, tilemap, dirty.layer, dirty.cell);
        }
        for dirty in &summary.dirty_regions {
            self.refresh_rect(scene, tilemap, dirty.layer, dirty.rect);
        }
        Ok(())
    }

    fn layer_splits_match(&self, scene: &TileMap, layer_ids: &[LayerId]) -> bool {
        layer_ids.iter().all(|layer_id| {
            let expected_palettes = TileMapRenderSync::palettes_for_layer(scene, *layer_id);
            let existing_palettes = self
                .layers
                .iter()
                .filter(|layer| layer.source_layer == *layer_id)
                .map(|layer| layer.palette)
                .collect::<Vec<_>>();
            expected_palettes == existing_palettes
        })
    }

    fn refresh_cell(
        &self,
        scene: &TileMap,
        tilemap: &mut crate::render::Tilemap,
        layer_id: LayerId,
        cell: CellCoord,
    ) {
        for layer in self
            .layers
            .iter()
            .filter(|layer| layer.source_layer == layer_id)
        {
            TileMapRenderSync::clear_cell_in_tilemap(scene, tilemap, layer.storage_layer, cell);
            TileMapRenderSync::write_cell_to_tilemap(
                scene,
                tilemap,
                layer.source_layer,
                layer.palette,
                layer.storage_layer,
                cell,
            );
        }
    }

    fn refresh_rect(
        &self,
        scene: &TileMap,
        tilemap: &mut crate::render::Tilemap,
        layer_id: LayerId,
        rect: CellRect,
    ) {
        for layer in self
            .layers
            .iter()
            .filter(|layer| layer.source_layer == layer_id)
        {
            for cell in rect.cells() {
                TileMapRenderSync::clear_cell_in_tilemap(scene, tilemap, layer.storage_layer, cell);
            }
            TileMapRenderSync::write_rect_to_tilemap(
                scene,
                tilemap,
                layer.source_layer,
                layer.palette,
                layer.storage_layer,
                rect,
            );
        }
    }
}

fn normalized_layers(layers: impl IntoIterator<Item = LayerId>) -> Vec<LayerId> {
    let mut layers = layers.into_iter().collect::<Vec<_>>();
    layers.sort();
    layers.dedup();
    layers
}
