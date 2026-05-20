use super::types::{PaletteId, TilePalette};

/// Loaded palette collection.
#[derive(Clone, Debug, Default)]
pub struct TilePaletteStore {
    palettes: Vec<TilePalette>,
}

impl TilePaletteStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, palette: TilePalette) {
        if let Some(existing) = self
            .palettes
            .iter_mut()
            .find(|existing| existing.id == palette.id)
        {
            *existing = palette;
        } else {
            self.palettes.push(palette);
        }
    }

    pub fn get(&self, id: PaletteId) -> Option<&TilePalette> {
        self.palettes.iter().find(|palette| palette.id == id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &TilePalette> {
        self.palettes.iter()
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.palettes.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.palettes.is_empty()
    }
}

impl Extend<TilePalette> for TilePaletteStore {
    fn extend<T: IntoIterator<Item = TilePalette>>(&mut self, iter: T) {
        for palette in iter {
            self.insert(palette);
        }
    }
}

impl FromIterator<TilePalette> for TilePaletteStore {
    fn from_iter<T: IntoIterator<Item = TilePalette>>(iter: T) -> Self {
        let mut store = Self::new();
        store.extend(iter);
        store
    }
}
