use crate::render::component::MAX_DIRECTIONAL_SHADOW_CASCADES;

pub(crate) const DEFAULT_SHADOW_ATLAS_GUARD_BAND_TEXELS: f32 = 1.0;
pub(crate) const DEFAULT_SHADOW_ATLAS_MAX_SIZE: u32 = 8192;
pub(crate) const MAX_SPOT_SHADOW_RECTS: usize = 8;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ShadowAtlasRect {
    pub(crate) x: u32,
    pub(crate) y: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

impl ShadowAtlasRect {
    #[inline]
    pub(crate) const fn area(self) -> u64 {
        self.width as u64 * self.height as u64
    }

    #[inline]
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) const fn right(self) -> u32 {
        self.x.saturating_add(self.width)
    }

    #[inline]
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) const fn bottom(self) -> u32 {
        self.y.saturating_add(self.height)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ShadowAtlasRequest {
    id: u32,
    width: u32,
    height: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ShadowAtlasAllocation {
    pub(crate) id: u32,
    pub(crate) rect: ShadowAtlasRect,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ShadowAtlasAllocator {
    requests: Vec<ShadowAtlasRequest>,
    min_width: u32,
    min_height: u32,
}

impl ShadowAtlasAllocator {
    #[inline]
    pub(crate) fn add_rect(&mut self, id: u32, width: u32, height: u32) {
        let width = width.max(1);
        let height = height.max(1);
        self.requests.push(ShadowAtlasRequest { id, width, height });
        self.min_width = self.min_width.max(width);
        self.min_height = self.min_height.max(height);
    }

    pub(crate) fn pack(&self, max_size: u32) -> Option<ShadowAtlasPacking> {
        if self.requests.is_empty() {
            return None;
        }
        let max_size = max_size.max(1);
        let mut width = self.min_width.max(1);
        let mut height = self.min_height.max(1);
        while width <= max_size && height <= max_size {
            if let Some(allocations) = self.try_pack(width, height) {
                return Some(ShadowAtlasPacking {
                    atlas_size: [width, height],
                    allocations,
                });
            }
            if height < width {
                height = height.saturating_mul(2);
            } else {
                width = width.saturating_mul(2);
            }
        }
        None
    }

    fn try_pack(&self, width: u32, height: u32) -> Option<Vec<ShadowAtlasAllocation>> {
        let mut order = (0..self.requests.len()).collect::<Vec<_>>();
        order.sort_by(|&lhs, &rhs| {
            let lhs_req = self.requests[lhs];
            let rhs_req = self.requests[rhs];
            rhs_req
                .height
                .cmp(&lhs_req.height)
                .then_with(|| rhs_req.width.cmp(&lhs_req.width))
                .then_with(|| lhs_req.id.cmp(&rhs_req.id))
        });

        let mut allocations = Vec::with_capacity(self.requests.len());
        let mut cursor_x = 0u32;
        let mut cursor_y = 0u32;
        let mut row_height = 0u32;
        for index in order {
            let request = self.requests[index];
            if request.width > width || request.height > height {
                return None;
            }
            if cursor_x.saturating_add(request.width) > width {
                cursor_y = cursor_y.saturating_add(row_height);
                cursor_x = 0;
                row_height = 0;
            }
            if cursor_y.saturating_add(request.height) > height {
                return None;
            }
            allocations.push(ShadowAtlasAllocation {
                id: request.id,
                rect: ShadowAtlasRect {
                    x: cursor_x,
                    y: cursor_y,
                    width: request.width,
                    height: request.height,
                },
            });
            cursor_x = cursor_x.saturating_add(request.width);
            row_height = row_height.max(request.height);
        }
        allocations.sort_by_key(|allocation| allocation.id);
        Some(allocations)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ShadowAtlasPacking {
    pub(crate) atlas_size: [u32; 2],
    pub(crate) allocations: Vec<ShadowAtlasAllocation>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct ShadowAtlasStats {
    pub(crate) active_cascade_count: u32,
    pub(crate) atlas_size: [u32; 2],
    pub(crate) used_rect_count: u32,
    pub(crate) used_pixel_ratio: f32,
    pub(crate) guard_band_texels: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ShadowAtlasLayout {
    resolution: u32,
    cascade_count: u32,
    spot_count: u32,
    atlas_size: [u32; 2],
    directional_rect: ShadowAtlasRect,
    spot_rects: [ShadowAtlasRect; MAX_SPOT_SHADOW_RECTS],
    guard_band_texels: f32,
}

impl ShadowAtlasLayout {
    #[inline]
    pub(crate) fn directional_packed(
        resolution: u32,
        cascade_count: u32,
        guard_band_texels: f32,
    ) -> Self {
        let resolution = resolution.max(1);
        let cascade_count = cascade_count
            .max(1)
            .min(MAX_DIRECTIONAL_SHADOW_CASCADES as u32);
        let mut allocator = ShadowAtlasAllocator::default();
        allocator.add_rect(0, resolution.saturating_mul(cascade_count), resolution);
        let packing = allocator
            .pack(DEFAULT_SHADOW_ATLAS_MAX_SIZE)
            .expect("single directional shadow rect should fit within the default atlas limit");
        let directional_rect = packing
            .allocations
            .iter()
            .find(|allocation| allocation.id == 0)
            .map(|allocation| allocation.rect)
            .expect("directional shadow rect allocation should be present");
        Self {
            resolution,
            cascade_count,
            spot_count: 0,
            atlas_size: packing.atlas_size,
            directional_rect,
            spot_rects: [ShadowAtlasRect::default(); MAX_SPOT_SHADOW_RECTS],
            guard_band_texels: guard_band_texels.max(0.0),
        }
    }

    #[inline]
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn directional_and_spots(
        directional_resolution: u32,
        cascade_count: u32,
        spot_resolution: u32,
        spot_count: u32,
        guard_band_texels: f32,
    ) -> Self {
        let directional_resolution = directional_resolution.max(1);
        let spot_resolution = spot_resolution.max(1);
        let cascade_count = cascade_count
            .max(1)
            .min(MAX_DIRECTIONAL_SHADOW_CASCADES as u32);
        let spot_count = spot_count.min(MAX_SPOT_SHADOW_RECTS as u32);
        let mut allocator = ShadowAtlasAllocator::default();
        allocator.add_rect(
            0,
            directional_resolution.saturating_mul(cascade_count),
            directional_resolution,
        );
        for spot in 0..spot_count {
            allocator.add_rect(spot + 1, spot_resolution, spot_resolution);
        }
        let packing = allocator
            .pack(DEFAULT_SHADOW_ATLAS_MAX_SIZE)
            .expect("directional and spot shadow rects should fit within the default atlas limit");
        let mut directional_rect = ShadowAtlasRect::default();
        let mut spot_rects = [ShadowAtlasRect::default(); MAX_SPOT_SHADOW_RECTS];
        for allocation in &packing.allocations {
            if allocation.id == 0 {
                directional_rect = allocation.rect;
            } else {
                let index = (allocation.id - 1) as usize;
                if index < spot_rects.len() {
                    spot_rects[index] = allocation.rect;
                }
            }
        }
        Self {
            resolution: directional_resolution,
            cascade_count,
            spot_count,
            atlas_size: packing.atlas_size,
            directional_rect,
            spot_rects,
            guard_band_texels: guard_band_texels.max(0.0),
        }
    }

    #[inline]
    pub(crate) fn atlas_size(self) -> [u32; 2] {
        self.atlas_size
    }

    #[inline]
    pub(crate) fn cascade_rect(self, cascade: u32) -> ShadowAtlasRect {
        let cascade = cascade.min(self.cascade_count - 1);
        ShadowAtlasRect {
            x: self
                .directional_rect
                .x
                .saturating_add(cascade.saturating_mul(self.resolution)),
            y: self.directional_rect.y,
            width: self.resolution,
            height: self.resolution,
        }
    }

    #[inline]
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) const fn spot_count(self) -> u32 {
        self.spot_count
    }

    #[inline]
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn spot_rect(self, spot_index: u32) -> Option<ShadowAtlasRect> {
        if spot_index >= self.spot_count {
            return None;
        }
        Some(self.spot_rects[spot_index as usize])
    }

    #[inline]
    pub(crate) fn shadow_atlas_mul_add(self) -> [f32; 4] {
        [
            self.resolution as f32 / self.atlas_size[0].max(1) as f32,
            self.resolution as f32 / self.atlas_size[1].max(1) as f32,
            self.directional_rect.x as f32 / self.atlas_size[0].max(1) as f32,
            self.directional_rect.y as f32 / self.atlas_size[1].max(1) as f32,
        ]
    }

    #[inline]
    pub(crate) fn shadow_atlas_resolution_rcp(self) -> [f32; 4] {
        [
            (self.atlas_size[0] as f32).recip(),
            (self.atlas_size[1] as f32).recip(),
            self.guard_band_texels,
            0.0,
        ]
    }

    #[inline]
    pub(crate) fn stats(self) -> ShadowAtlasStats {
        let atlas_area = (self.atlas_size[0] as u64 * self.atlas_size[1] as u64).max(1);
        let spot_area = self
            .spot_rects
            .iter()
            .take(self.spot_count as usize)
            .map(|rect| rect.area())
            .sum::<u64>();
        ShadowAtlasStats {
            active_cascade_count: self.cascade_count,
            atlas_size: self.atlas_size,
            used_rect_count: 1 + self.spot_count,
            used_pixel_ratio: (self.directional_rect.area() + spot_area) as f32 / atlas_area as f32,
            guard_band_texels: self.guard_band_texels,
        }
    }
}

impl Default for ShadowAtlasLayout {
    fn default() -> Self {
        Self::directional_packed(1, 1, DEFAULT_SHADOW_ATLAS_GUARD_BAND_TEXELS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packed_directional_layout_matches_wicked_slice_mapping() {
        let layout = ShadowAtlasLayout::directional_packed(512, 4, 1.0);

        assert_eq!(layout.atlas_size(), [2048, 512]);
        assert_eq!(layout.shadow_atlas_mul_add(), [0.25, 1.0, 0.0, 0.0]);
        assert_eq!(
            layout.shadow_atlas_resolution_rcp(),
            [1.0 / 2048.0, 1.0 / 512.0, 1.0, 0.0]
        );
        assert_eq!(
            layout.cascade_rect(2),
            ShadowAtlasRect {
                x: 1024,
                y: 0,
                width: 512,
                height: 512
            }
        );
    }

    #[test]
    fn packed_directional_layout_reports_atlas_usage_stats() {
        let stats = ShadowAtlasLayout::directional_packed(128, 3, 2.0).stats();

        assert_eq!(stats.active_cascade_count, 3);
        assert_eq!(stats.atlas_size, [384, 128]);
        assert_eq!(stats.used_rect_count, 1);
        assert_eq!(stats.used_pixel_ratio, 1.0);
        assert_eq!(stats.guard_band_texels, 2.0);
    }

    #[test]
    fn atlas_allocator_grows_like_wicked_rectpacker_state() {
        let mut allocator = ShadowAtlasAllocator::default();
        allocator.add_rect(20, 128, 64);
        allocator.add_rect(10, 64, 64);

        let packing = allocator
            .pack(512)
            .expect("small shadow rects should fit in a grown atlas");

        assert_eq!(packing.atlas_size, [128, 128]);
        assert_eq!(packing.allocations.len(), 2);
        assert_eq!(packing.allocations[0].id, 10);
        assert_eq!(packing.allocations[1].id, 20);
        assert!(
            packing.allocations[0].rect.bottom() <= packing.atlas_size[1]
                && packing.allocations[1].rect.right() <= packing.atlas_size[0]
        );
    }

    #[test]
    fn layout_packs_directional_and_spot_shadow_rects() {
        let layout = ShadowAtlasLayout::directional_and_spots(512, 4, 256, 2, 1.0);
        let stats = layout.stats();

        assert_eq!(layout.atlas_size(), [2048, 1024]);
        assert_eq!(layout.shadow_atlas_mul_add(), [0.25, 0.5, 0.0, 0.0]);
        assert_eq!(layout.spot_count(), 2);
        assert_eq!(
            layout.spot_rect(0),
            Some(ShadowAtlasRect {
                x: 0,
                y: 512,
                width: 256,
                height: 256,
            })
        );
        assert_eq!(stats.used_rect_count, 3);
        assert!(stats.used_pixel_ratio > 0.5 && stats.used_pixel_ratio < 1.0);
    }
}
