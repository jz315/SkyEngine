use crate::render::Color;

bitflags::bitflags! {
    /// Per-tile texture transform flags.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct TileFlags: u8 {
        const FLIP_X = 1 << 0;
        const FLIP_Y = 1 << 1;
        const FLIP_DIAGONAL = 1 << 2;
    }
}

/// Index of a tile inside a [`crate::render::TilesetGrid`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TileId(pub u32);

impl TileId {
    pub const EMPTY: Self = Self(u32::MAX);

    #[inline]
    pub const fn is_empty(self) -> bool {
        self.0 == Self::EMPTY.0
    }
}

/// One cell in a tilemap layer.
#[derive(Clone, Copy, Debug)]
pub struct Tile {
    pub id: TileId,
    pub tint: Color,
    pub flags: TileFlags,
}

impl Tile {
    pub const EMPTY: Self = Self {
        id: TileId::EMPTY,
        tint: Color::TRANSPARENT,
        flags: TileFlags::empty(),
    };

    #[inline]
    pub const fn new(id: TileId) -> Self {
        Self {
            id,
            tint: Color::WHITE,
            flags: TileFlags::empty(),
        }
    }

    #[inline]
    pub const fn tinted(id: TileId, tint: Color) -> Self {
        Self {
            id,
            tint,
            flags: TileFlags::empty(),
        }
    }

    #[inline]
    pub const fn with_flags(mut self, flags: TileFlags) -> Self {
        self.flags = flags;
        self
    }

    #[inline]
    pub const fn is_empty(self) -> bool {
        self.id.is_empty()
    }
}

impl Default for Tile {
    fn default() -> Self {
        Self::EMPTY
    }
}

/// Stable handle for tilemap data stored in [`TilemapStorage`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TilemapHandle {
    index: u32,
    generation: u32,
}

impl TilemapHandle {
    #[inline]
    pub const fn new(index: u32, generation: u32) -> Self {
        Self { index, generation }
    }

    #[inline]
    pub const fn index(self) -> u32 {
        self.index
    }

    #[inline]
    pub const fn generation(self) -> u32 {
        self.generation
    }
}

/// Creation parameters for a tilemap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TilemapDescriptor {
    pub width: u32,
    pub height: u32,
    pub layers: u32,
    pub chunk_size: [u32; 2],
}

impl TilemapDescriptor {
    #[inline]
    pub const fn new(width: u32, height: u32, layers: u32) -> Self {
        Self {
            width,
            height,
            layers,
            chunk_size: [32, 32],
        }
    }

    #[inline]
    pub const fn chunk_size(mut self, chunk_size: [u32; 2]) -> Self {
        self.chunk_size = chunk_size;
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TileChunkBounds {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug)]
struct TileChunk {
    version: u64,
    non_empty: u32,
}

impl TileChunk {
    #[inline]
    fn new() -> Self {
        Self {
            version: 1,
            non_empty: 0,
        }
    }

    #[inline]
    fn mark_changed(&mut self) {
        self.version = self.version.wrapping_add(1).max(1);
    }
}

#[derive(Clone, Debug)]
struct TileLayer {
    tiles: Vec<Tile>,
    chunks: Vec<TileChunk>,
}

impl TileLayer {
    fn new(tile_count: usize, chunk_count: usize) -> Self {
        Self {
            tiles: vec![Tile::EMPTY; tile_count],
            chunks: (0..chunk_count).map(|_| TileChunk::new()).collect(),
        }
    }
}

/// Chunked, layered tilemap data.
#[derive(Clone, Debug)]
pub struct Tilemap {
    width: u32,
    height: u32,
    layers: Vec<TileLayer>,
    chunk_size: [u32; 2],
    chunk_columns: u32,
    chunk_rows: u32,
}

impl Tilemap {
    pub fn new(desc: TilemapDescriptor) -> Self {
        let width = desc.width.max(1);
        let height = desc.height.max(1);
        let layers = desc.layers.max(1);
        let chunk_size = [desc.chunk_size[0].max(1), desc.chunk_size[1].max(1)];
        let chunk_columns = width.div_ceil(chunk_size[0]);
        let chunk_rows = height.div_ceil(chunk_size[1]);
        let tile_count = width as usize * height as usize;
        let chunk_count = chunk_columns as usize * chunk_rows as usize;
        Self {
            width,
            height,
            layers: (0..layers)
                .map(|_| TileLayer::new(tile_count, chunk_count))
                .collect(),
            chunk_size,
            chunk_columns,
            chunk_rows,
        }
    }

    #[inline]
    pub fn width(&self) -> u32 {
        self.width
    }

    #[inline]
    pub fn height(&self) -> u32 {
        self.height
    }

    #[inline]
    pub fn layer_count(&self) -> u32 {
        self.layers.len() as u32
    }

    #[inline]
    pub fn chunk_size(&self) -> [u32; 2] {
        self.chunk_size
    }

    #[inline]
    pub fn chunk_columns(&self) -> u32 {
        self.chunk_columns
    }

    #[inline]
    pub fn chunk_rows(&self) -> u32 {
        self.chunk_rows
    }

    #[inline]
    pub fn tile(&self, layer: u32, x: u32, y: u32) -> Option<Tile> {
        let index = self.tile_index(x, y)?;
        self.layers
            .get(layer as usize)
            .and_then(|layer| layer.tiles.get(index).copied())
    }

    pub fn set_tile(&mut self, layer: u32, x: u32, y: u32, tile: Tile) -> Option<Tile> {
        let tile_index = self.tile_index(x, y)?;
        let chunk_index = self.chunk_index_for_tile(x, y)?;
        let layer = self.layers.get_mut(layer as usize)?;
        let old = layer.tiles[tile_index];
        if old.id == tile.id
            && old.flags == tile.flags
            && old.tint.to_array() == tile.tint.to_array()
        {
            return Some(old);
        }

        let old_empty = old.is_empty();
        let new_empty = tile.is_empty();
        if old_empty != new_empty {
            let chunk = &mut layer.chunks[chunk_index];
            if new_empty {
                chunk.non_empty = chunk.non_empty.saturating_sub(1);
            } else {
                chunk.non_empty = chunk.non_empty.saturating_add(1);
            }
        }
        layer.tiles[tile_index] = tile;
        layer.chunks[chunk_index].mark_changed();
        Some(old)
    }

    pub fn fill_rect(&mut self, layer: u32, x: u32, y: u32, width: u32, height: u32, tile: Tile) {
        let max_x = x.saturating_add(width).min(self.width);
        let max_y = y.saturating_add(height).min(self.height);
        for yy in y..max_y {
            for xx in x..max_x {
                let _ = self.set_tile(layer, xx, yy, tile);
            }
        }
    }

    pub fn clear_layer(&mut self, layer: u32) {
        let Some(layer) = self.layers.get_mut(layer as usize) else {
            return;
        };
        layer.tiles.fill(Tile::EMPTY);
        for chunk in &mut layer.chunks {
            chunk.non_empty = 0;
            chunk.mark_changed();
        }
    }

    pub fn chunk_bounds(&self, chunk_x: u32, chunk_y: u32) -> Option<TileChunkBounds> {
        if chunk_x >= self.chunk_columns || chunk_y >= self.chunk_rows {
            return None;
        }
        let x = chunk_x * self.chunk_size[0];
        let y = chunk_y * self.chunk_size[1];
        Some(TileChunkBounds {
            x,
            y,
            width: self.chunk_size[0].min(self.width - x),
            height: self.chunk_size[1].min(self.height - y),
        })
    }

    pub fn chunk_version(&self, layer: u32, chunk_x: u32, chunk_y: u32) -> Option<u64> {
        let chunk_index = self.chunk_index(chunk_x, chunk_y)?;
        self.layers
            .get(layer as usize)
            .and_then(|layer| layer.chunks.get(chunk_index))
            .map(|chunk| chunk.version)
    }

    pub fn chunk_non_empty_tiles(&self, layer: u32, chunk_x: u32, chunk_y: u32) -> Option<u32> {
        let chunk_index = self.chunk_index(chunk_x, chunk_y)?;
        self.layers
            .get(layer as usize)
            .and_then(|layer| layer.chunks.get(chunk_index))
            .map(|chunk| chunk.non_empty)
    }

    #[inline]
    fn tile_index(&self, x: u32, y: u32) -> Option<usize> {
        if x >= self.width || y >= self.height {
            return None;
        }
        Some((y * self.width + x) as usize)
    }

    #[inline]
    fn chunk_index_for_tile(&self, x: u32, y: u32) -> Option<usize> {
        self.chunk_index(x / self.chunk_size[0], y / self.chunk_size[1])
    }

    #[inline]
    fn chunk_index(&self, chunk_x: u32, chunk_y: u32) -> Option<usize> {
        if chunk_x >= self.chunk_columns || chunk_y >= self.chunk_rows {
            return None;
        }
        Some((chunk_y * self.chunk_columns + chunk_x) as usize)
    }
}

/// Resource that owns tilemap payloads outside ECS component storage.
#[derive(Default)]
pub struct TilemapStorage {
    maps: Vec<Option<Tilemap>>,
    generations: Vec<u32>,
    free_list: Vec<u32>,
    len: usize,
}

impl TilemapStorage {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn create(&mut self, desc: TilemapDescriptor) -> TilemapHandle {
        self.insert(Tilemap::new(desc))
    }

    pub fn insert(&mut self, map: Tilemap) -> TilemapHandle {
        let index = if let Some(index) = self.free_list.pop() {
            self.maps[index as usize] = Some(map);
            index
        } else {
            let index = self.maps.len() as u32;
            self.maps.push(Some(map));
            self.generations.push(0);
            index
        };
        self.len += 1;
        TilemapHandle::new(index, self.generations[index as usize])
    }

    pub fn get(&self, handle: TilemapHandle) -> Option<&Tilemap> {
        if self.generations.get(handle.index as usize).copied()? != handle.generation {
            return None;
        }
        self.maps.get(handle.index as usize)?.as_ref()
    }

    pub fn get_mut(&mut self, handle: TilemapHandle) -> Option<&mut Tilemap> {
        if self.generations.get(handle.index as usize).copied()? != handle.generation {
            return None;
        }
        self.maps.get_mut(handle.index as usize)?.as_mut()
    }

    pub fn remove(&mut self, handle: TilemapHandle) -> Option<Tilemap> {
        if self.generations.get(handle.index as usize).copied()? != handle.generation {
            return None;
        }
        let map = self.maps.get_mut(handle.index as usize)?.take()?;
        self.generations[handle.index as usize] =
            self.generations[handle.index as usize].wrapping_add(1);
        self.free_list.push(handle.index);
        self.len -= 1;
        Some(map)
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

#[cfg(test)]
mod tests {
    use super::{Tile, TileId, Tilemap, TilemapDescriptor};

    #[test]
    fn set_tile_updates_chunk_occupancy_and_version() {
        let mut map = Tilemap::new(TilemapDescriptor::new(8, 8, 1).chunk_size([4, 4]));
        assert_eq!(map.chunk_non_empty_tiles(0, 0, 0), Some(0));
        let before = map.chunk_version(0, 0, 0).unwrap();

        assert!(map.set_tile(0, 2, 3, Tile::new(TileId(7))).is_some());

        assert_eq!(map.chunk_non_empty_tiles(0, 0, 0), Some(1));
        assert!(map.chunk_version(0, 0, 0).unwrap() > before);
    }

    #[test]
    fn remove_reuses_handle_slots_with_new_generation() {
        let mut storage = super::TilemapStorage::new();
        let first = storage.create(TilemapDescriptor::new(4, 4, 1));
        assert!(storage.remove(first).is_some());
        let second = storage.create(TilemapDescriptor::new(4, 4, 1));

        assert_eq!(first.index(), second.index());
        assert_ne!(first.generation(), second.generation());
        assert!(storage.get(first).is_none());
        assert!(storage.get(second).is_some());
    }

    #[test]
    fn layers_store_tiles_independently() {
        let mut map = Tilemap::new(TilemapDescriptor::new(4, 4, 3));

        assert!(map.set_tile(0, 1, 1, Tile::new(TileId(1))).is_some());
        assert!(map.set_tile(2, 1, 1, Tile::new(TileId(7))).is_some());

        assert_eq!(map.tile(0, 1, 1).unwrap().id, TileId(1));
        assert_eq!(map.tile(1, 1, 1).unwrap().id, TileId::EMPTY);
        assert_eq!(map.tile(2, 1, 1).unwrap().id, TileId(7));

        map.clear_layer(2);

        assert_eq!(map.tile(0, 1, 1).unwrap().id, TileId(1));
        assert_eq!(map.tile(2, 1, 1).unwrap().id, TileId::EMPTY);
    }
}
