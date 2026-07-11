use super::*;

impl TiledImport {
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, TiledImportError> {
        let path = path.as_ref();
        match path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str()
        {
            "json" | "tmj" => Self::from_json_file(path),
            "tmx" => Self::from_tmx_file(path),
            _ => Err(TiledImportError::UnsupportedFileExtension {
                path: path.to_path_buf(),
            }),
        }
    }

    pub fn from_json_file(path: impl AsRef<Path>) -> Result<Self, TiledImportError> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path).map_err(|source| TiledImportError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let base_dir = path.parent().unwrap_or_else(|| Path::new("."));
        Self::from_json_str(&text, base_dir)
    }

    pub fn from_json_str(text: &str, base_dir: impl AsRef<Path>) -> Result<Self, TiledImportError> {
        let raw: TiledJsonMap = serde_json::from_str(text).map_err(TiledImportError::Json)?;
        Self::from_raw(raw, base_dir.as_ref())
    }

    pub fn from_tmx_file(path: impl AsRef<Path>) -> Result<Self, TiledImportError> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path).map_err(|source| TiledImportError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let base_dir = path.parent().unwrap_or_else(|| Path::new("."));
        Self::from_tmx_str(&text, base_dir)
    }

    pub fn from_tmx_str(text: &str, base_dir: impl AsRef<Path>) -> Result<Self, TiledImportError> {
        let document = roxmltree::Document::parse(text).map_err(TiledImportError::Xml)?;
        let root = document.root_element();
        if !root.has_tag_name("map") {
            return Err(TiledImportError::MalformedMap(
                "TMX root element must be <map>".to_string(),
            ));
        }

        let orientation = parse_orientation(required_attr(root, "orientation")?)?;
        let stagger_axis = parse_stagger_axis(root.attribute("staggeraxis").unwrap_or("y"))?;
        let stagger_index = parse_stagger_index(root.attribute("staggerindex").unwrap_or("odd"))?;
        let render_order =
            parse_render_order(root.attribute("renderorder").unwrap_or("right-down"))?;
        let width = required_u32_attr(root, "width")?;
        let height = required_u32_attr(root, "height")?;
        let tile_width = required_u32_attr(root, "tilewidth")?;
        let tile_height = required_u32_attr(root, "tileheight")?;
        let hex_side_length = optional_u32_attr(root, "hexsidelength")?.unwrap_or_default();
        let infinite = optional_bool_attr(root, "infinite")?.unwrap_or(false);
        let parallax_origin = [
            optional_f32_attr(root, "parallaxoriginx")?.unwrap_or_default(),
            optional_f32_attr(root, "parallaxoriginy")?.unwrap_or_default(),
        ];
        let tilesets = resolve_tmx_tilesets(root, base_dir.as_ref())?;

        let mut layers = Vec::new();
        let mut object_layers = Vec::new();
        let mut source_order = 0i32;
        collect_tmx_child_layers(
            root,
            LayerContext::default(),
            &mut source_order,
            &mut layers,
            &mut object_layers,
            base_dir.as_ref(),
        )?;

        Self::from_parts(ParsedMap {
            orientation,
            stagger_axis,
            stagger_index,
            hex_side_length,
            render_order,
            width,
            height,
            tile_width,
            tile_height,
            infinite,
            parallax_origin,
            properties: collect_tmx_properties(root, base_dir.as_ref())?,
            layers,
            object_layers,
            tilesets,
        })
    }

    pub fn primary_tileset(&self) -> &TiledTileset {
        self.tilesets.first().unwrap_or(&self.tileset)
    }

    pub fn load_tileset_texture(&self) -> Result<TextureAsset, TiledImportError> {
        load_tiled_tileset_texture(self.primary_tileset())
    }

    pub fn load_tileset_textures(&self) -> Result<Vec<TextureAsset>, TiledImportError> {
        self.tilesets
            .iter()
            .map(load_tiled_tileset_texture)
            .collect()
    }

    pub(crate) fn tileset_grid_for(
        &self,
        tileset_index: usize,
        texture: Handle<TextureAsset>,
    ) -> Option<TilesetGrid> {
        let tileset = self.tilesets.get(tileset_index)?;
        Some(
            TilesetGrid::new(texture, tileset.tile_size, tileset.columns, tileset.rows)
                .texture_size(tileset.image_size)
                .margin_spacing(tileset.margin, tileset.spacing)
                .tile_rects(tileset.tile_rects.clone())
                .animations(tileset.animations.clone()),
        )
    }

    #[inline]
    pub fn tileset_grid(&self, texture: Handle<TextureAsset>) -> TilesetGrid {
        self.tileset_grid_for(0, texture.clone())
            .unwrap_or_else(|| {
                TilesetGrid::new(
                    texture,
                    self.tileset.tile_size,
                    self.tileset.columns,
                    self.tileset.rows,
                )
                .texture_size(self.tileset.image_size)
                .margin_spacing(self.tileset.margin, self.tileset.spacing)
                .tile_rects(self.tileset.tile_rects.clone())
                .animations(self.tileset.animations.clone())
            })
    }

    pub fn renderer_for_layer(
        &self,
        map: TilemapHandle,
        textures: &[Handle<TextureAsset>],
        layer: u32,
    ) -> Option<TilemapRenderer> {
        let layer_info = self.layers.get(layer as usize)?;
        let texture = textures.get(layer_info.tileset_index)?.clone();
        let tileset = self.tilesets.get(layer_info.tileset_index)?;
        let mut renderer = TilemapRenderer::new(
            map,
            self.tileset_grid_for(layer_info.tileset_index, texture)?,
        )
        .layer(layer_info.storage_layer)
        .tile_size([self.tile_size[0] as f32, self.tile_size[1] as f32])
        .tile_draw_size([tileset.tile_size[0] as f32, tileset.tile_size[1] as f32])
        .tile_offset([
            tileset.tile_offset[0] as f32,
            -tileset.tile_offset[1] as f32,
        ])
        .orientation(self.orientation)
        .stagger_axis(self.stagger_axis)
        .stagger_index(self.renderer_stagger_index())
        .hex_side_length(self.hex_side_length as f32)
        .render_order(self.render_order)
        .depth_sort(self.depth_sort)
        .visible(layer_info.visible);
        if layer_info.opacity < 1.0 {
            renderer = renderer.color(Color::new(1.0, 1.0, 1.0, layer_info.opacity));
        }
        Some(renderer)
    }

    pub fn renderer_for_layer_with_texture(
        &self,
        map: TilemapHandle,
        texture: Handle<TextureAsset>,
        layer: u32,
    ) -> Option<TilemapRenderer> {
        self.renderer_for_layer(map, &[texture], layer)
    }
}

fn load_tiled_tileset_texture(tileset: &TiledTileset) -> Result<TextureAsset, TiledImportError> {
    if !tileset.tile_images.is_empty() {
        return load_tiled_image_collection_atlas(tileset);
    }

    let image = load_tiled_rgba_image(&tileset.image, tileset.transparent_color)?;
    let (width, height) = image.dimensions();
    Ok(TextureAsset::new(
        width,
        height,
        TextureColorSpace::Srgb,
        image.into_raw(),
    ))
}

fn load_tiled_rgba_image(
    path: &Path,
    transparent_color: Option<[u8; 3]>,
) -> Result<image::RgbaImage, TiledImportError> {
    let image = image::open(path).map_err(|source| TiledImportError::Image {
        path: path.to_path_buf(),
        source,
    })?;
    let mut image = image.to_rgba8();
    if let Some([r, g, b]) = transparent_color {
        for pixel in image.as_flat_samples_mut().samples.chunks_exact_mut(4) {
            if pixel[0] == r && pixel[1] == g && pixel[2] == b {
                pixel[3] = 0;
            }
        }
    }
    Ok(image)
}

fn load_tiled_image_collection_atlas(
    tileset: &TiledTileset,
) -> Result<TextureAsset, TiledImportError> {
    let [atlas_width, atlas_height] = [tileset.image_size[0].max(1), tileset.image_size[1].max(1)];
    let mut atlas_pixels = vec![0u8; (atlas_width * atlas_height * 4) as usize];
    for (tile_id, source) in tileset.tile_images.iter().enumerate() {
        let Some(source) = source else {
            continue;
        };
        let atlas_rect = tileset
            .tile_rects
            .get(tile_id)
            .and_then(|rect| *rect)
            .ok_or_else(|| TiledImportError::UnsupportedTileset {
                source: Some(source.image.clone()),
                reason: "image collection atlas tile is missing a packed rectangle",
            })?;
        let image = load_tiled_rgba_image(&source.image, tileset.transparent_color)?;
        blit_tiled_image_rect(
            &image,
            source.source_rect,
            &mut atlas_pixels,
            [atlas_width, atlas_height],
            atlas_rect,
            &source.image,
        )?;
    }

    Ok(TextureAsset::new(
        atlas_width,
        atlas_height,
        TextureColorSpace::Srgb,
        atlas_pixels,
    ))
}

fn blit_tiled_image_rect(
    image: &image::RgbaImage,
    source_rect: TilesetTileRect,
    atlas_pixels: &mut [u8],
    atlas_size: [u32; 2],
    atlas_rect: TilesetTileRect,
    image_path: &Path,
) -> Result<(), TiledImportError> {
    if source_rect.x.saturating_add(source_rect.width) > image.width()
        || source_rect.y.saturating_add(source_rect.height) > image.height()
    {
        return Err(TiledImportError::UnsupportedTileset {
            source: Some(image_path.to_path_buf()),
            reason: "image collection tile source rectangle is outside the source image",
        });
    }
    if atlas_rect.x.saturating_add(atlas_rect.width) > atlas_size[0]
        || atlas_rect.y.saturating_add(atlas_rect.height) > atlas_size[1]
        || atlas_rect.width != source_rect.width
        || atlas_rect.height != source_rect.height
    {
        return Err(TiledImportError::UnsupportedTileset {
            source: Some(image_path.to_path_buf()),
            reason: "image collection atlas rectangle is invalid",
        });
    }

    let source_row_stride = image.width() as usize * 4;
    let atlas_row_stride = atlas_size[0] as usize * 4;
    let pixels = image.as_raw();
    let copy_len = source_rect.width as usize * 4;
    for row in 0..source_rect.height {
        let source_start =
            ((source_rect.y + row) as usize * source_row_stride) + source_rect.x as usize * 4;
        let source_end = source_start + copy_len;
        let atlas_start =
            ((atlas_rect.y + row) as usize * atlas_row_stride) + atlas_rect.x as usize * 4;
        let atlas_end = atlas_start + copy_len;
        atlas_pixels[atlas_start..atlas_end].copy_from_slice(&pixels[source_start..source_end]);
    }
    Ok(())
}
