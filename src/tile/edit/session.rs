mod objects;
mod properties;
mod revert;
mod tiles;

use super::super::scene::TileMap;
use super::summary::TileMapEditSummary;

/// Mutable edit session around a tile scene.
pub struct TileMapEditSession<'a> {
    pub(super) scene: &'a mut TileMap,
    pub(super) summary: TileMapEditSummary,
}

impl<'a> TileMapEditSession<'a> {
    pub fn new(scene: &'a mut TileMap) -> Self {
        Self {
            scene,
            summary: TileMapEditSummary::default(),
        }
    }

    pub fn scene(&self) -> &TileMap {
        self.scene
    }

    pub fn scene_mut(&mut self) -> &mut TileMap {
        self.scene
    }

    pub fn finish(self) -> TileMapEditSummary {
        self.summary
    }
}
