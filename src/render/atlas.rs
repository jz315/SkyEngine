//! Texture atlas for batching sprites with different images into a single
//! draw call.
//!
//! # Usage
//!
//! ```rust,ignore
//! let mut packer = AtlasPacker::new(2048, 2048);
//! packer.add("hero", 64, 64, &hero_pixels);
//! packer.add("enemy", 32, 32, &enemy_pixels);
//! let atlas = packer.build(gpu);
//!
//! // At draw time:
//! let uv = atlas.uv("hero").unwrap();
//! batch.set_texture(&atlas.texture());
//! batch.draw(Sprite::new(x, y, 64.0, 64.0).uv(uv));
//! ```

use std::borrow::Cow;

use rustc_hash::FxHashMap;

use crate::gpu::Gpu;
use crate::render::texture::Texture;

/// UV rectangle for a region within an atlas.
#[derive(Debug, Clone, Copy)]
pub struct UvRect {
    pub u_min: f32,
    pub v_min: f32,
    pub u_max: f32,
    pub v_max: f32,
}

impl UvRect {
    /// Convert to the `[u_min, v_min, u_max, v_max]` array used by `Sprite`.
    #[inline]
    pub fn to_array(self) -> [f32; 4] {
        [self.u_min, self.v_min, self.u_max, self.v_max]
    }
}

/// A packed entry waiting to be placed in the atlas.
struct PackEntry {
    name: Cow<'static, str>,
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

/// Rectangle returned by the shelf packer.
#[derive(Debug, Clone, Copy)]
struct PackedRect {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
}

/// Simple shelf-based rectangle packer.
///
/// Places rectangles left-to-right in rows (shelves). When a rectangle
/// doesn't fit on the current shelf, a new shelf is started below.
/// Entries are sorted tallest-first for better packing.
struct ShelfPacker {
    atlas_w: u32,
    atlas_h: u32,
    shelf_x: u32,
    shelf_y: u32,
    shelf_h: u32,
}

impl ShelfPacker {
    fn new(w: u32, h: u32) -> Self {
        Self {
            atlas_w: w,
            atlas_h: h,
            shelf_x: 0,
            shelf_y: 0,
            shelf_h: 0,
        }
    }

    fn pack(&mut self, w: u32, h: u32) -> Option<PackedRect> {
        // Padding between entries to avoid texture bleed.
        let pad = 1;
        let pw = w + pad;
        let ph = h + pad;

        // Entry wider or taller than the entire atlas → impossible.
        if pw > self.atlas_w || ph > self.atlas_h {
            return None;
        }

        // Doesn't fit on current shelf → start a new one.
        if self.shelf_x + pw > self.atlas_w {
            self.shelf_y += self.shelf_h;
            self.shelf_x = 0;
            self.shelf_h = 0;
        }

        // Doesn't fit vertically at all.
        if self.shelf_y + ph > self.atlas_h {
            return None;
        }

        let rect = PackedRect {
            x: self.shelf_x,
            y: self.shelf_y,
            w,
            h,
        };

        self.shelf_x += pw;
        self.shelf_h = self.shelf_h.max(ph);

        Some(rect)
    }
}

/// Builder for creating a [`TextureAtlas`].
pub struct AtlasPacker {
    width: u32,
    height: u32,
    entries: Vec<PackEntry>,
}

impl AtlasPacker {
    /// Create a new packer with the given atlas dimensions.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            entries: Vec::new(),
        }
    }

    /// Add a named sub-image to the atlas.
    ///
    /// `pixels` must be `width * height * 4` bytes (RGBA8).
    pub fn add(
        &mut self,
        name: impl Into<Cow<'static, str>>,
        width: u32,
        height: u32,
        pixels: &[u8],
    ) -> &mut Self {
        assert_eq!(
            pixels.len(),
            (width * height * 4) as usize,
            "pixel data size mismatch"
        );
        self.entries.push(PackEntry {
            name: name.into(),
            width,
            height,
            pixels: pixels.to_vec(),
        });
        self
    }

    /// Pack all entries and upload the atlas texture to the GPU.
    ///
    /// Returns `None` if the entries don't fit in the atlas dimensions.
    pub fn build(mut self, gpu: &mut impl Gpu) -> Option<TextureAtlas> {
        // Sort tallest-first for better shelf utilisation.
        self.entries
            .sort_by(|a, b| b.height.cmp(&a.height).then(b.width.cmp(&a.width)));

        let mut packer = ShelfPacker::new(self.width, self.height);
        let mut regions = FxHashMap::default();
        let mut atlas_pixels = vec![0u8; (self.width * self.height * 4) as usize];

        let atlas_w = self.width;
        let atlas_h = self.height;

        for entry in &self.entries {
            let rect = packer.pack(entry.width, entry.height)?;

            // Blit entry pixels into the atlas buffer.
            for row in 0..entry.height {
                let src_start = (row * entry.width * 4) as usize;
                let src_end = src_start + (entry.width * 4) as usize;
                let dst_start = ((rect.y + row) * atlas_w * 4 + rect.x * 4) as usize;
                let dst_end = dst_start + (entry.width * 4) as usize;
                atlas_pixels[dst_start..dst_end].copy_from_slice(&entry.pixels[src_start..src_end]);
            }

            // Compute UV coordinates.
            let uv = UvRect {
                u_min: rect.x as f32 / atlas_w as f32,
                v_min: rect.y as f32 / atlas_h as f32,
                u_max: (rect.x + rect.w) as f32 / atlas_w as f32,
                v_max: (rect.y + rect.h) as f32 / atlas_h as f32,
            };
            regions.insert(entry.name.clone(), uv);
        }

        // Create GPU texture.
        let texture = Texture::from_rgba8_with_label(gpu, atlas_w, atlas_h, &atlas_pixels, "atlas");

        Some(TextureAtlas { texture, regions })
    }
}

/// A packed texture atlas containing multiple named regions.
pub struct TextureAtlas {
    texture: Texture,
    regions: FxHashMap<Cow<'static, str>, UvRect>,
}

impl TextureAtlas {
    /// Get the atlas texture (for binding to a sprite batch).
    #[inline]
    pub fn texture(&self) -> &Texture {
        &self.texture
    }

    /// Look up a named region's UV rectangle.
    pub fn uv(&self, name: &str) -> Option<UvRect> {
        self.regions.get(name).copied()
    }

    /// Number of regions packed into the atlas.
    #[inline]
    pub fn region_count(&self) -> usize {
        self.regions.len()
    }

    /// Atlas texture width.
    #[inline]
    pub fn width(&self) -> u32 {
        self.texture.width()
    }

    /// Atlas texture height.
    #[inline]
    pub fn height(&self) -> u32 {
        self.texture.height()
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shelf_packer_fills_row() {
        let mut packer = ShelfPacker::new(64, 64);
        let r1 = packer.pack(30, 20).unwrap();
        assert_eq!((r1.x, r1.y), (0, 0));
        let r2 = packer.pack(30, 20).unwrap();
        assert_eq!((r2.x, r2.y), (31, 0)); // 30 + 1 pad

        // Third doesn't fit on row (31+30+1 = 62+1 = 63 ≤ 64?  no: 31+31=62 ≤ 64) let's check:
        // shelf_x = 31+31 = 62 after r2. r3: pw=31. 62+31=93 > 64 → new shelf
        let r3 = packer.pack(30, 10).unwrap();
        assert_eq!((r3.x, r3.y), (0, 21)); // shelf_h was 21 (20+1)
    }

    #[test]
    fn shelf_packer_rejects_overflow() {
        let mut packer = ShelfPacker::new(32, 32);
        assert!(packer.pack(33, 10).is_none()); // too wide
    }

    #[test]
    fn uv_rect_to_array() {
        let uv = UvRect {
            u_min: 0.0,
            v_min: 0.25,
            u_max: 0.5,
            v_max: 0.75,
        };
        assert_eq!(uv.to_array(), [0.0, 0.25, 0.5, 0.75]);
    }
}
