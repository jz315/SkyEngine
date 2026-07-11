use super::*;

impl TiledImport {
    pub(super) fn from_raw(raw: TiledJsonMap, base_dir: &Path) -> Result<Self, TiledImportError> {
        let orientation = parse_orientation(&raw.orientation)?;
        let stagger_axis = parse_stagger_axis(&raw.staggeraxis)?;
        let stagger_index = parse_stagger_index(&raw.staggerindex)?;
        let render_order = parse_render_order(&raw.renderorder)?;
        let parsed_tilesets = resolve_json_tilesets(&raw.tilesets, base_dir)?;

        let mut layers = Vec::new();
        let mut object_layers = Vec::new();
        let mut source_order = 0i32;
        collect_tile_layers(
            &raw.layers,
            LayerContext::default(),
            &mut source_order,
            &mut layers,
            &mut object_layers,
            base_dir,
        )?;

        Self::from_parts(ParsedMap {
            orientation,
            stagger_axis,
            stagger_index,
            hex_side_length: raw.hexsidelength,
            render_order,
            width: raw.width,
            height: raw.height,
            tile_width: raw.tilewidth,
            tile_height: raw.tileheight,
            infinite: raw.infinite,
            parallax_origin: [raw.parallaxoriginx, raw.parallaxoriginy],
            properties: collect_json_properties(&raw.properties, base_dir)?,
            layers,
            object_layers,
            tilesets: parsed_tilesets,
        })
    }

    pub(super) fn from_parts(parts: ParsedMap) -> Result<Self, TiledImportError> {
        if parts.tilesets.is_empty() {
            return Err(TiledImportError::MissingTileset);
        }

        let primary_tileset = parts.tilesets[0].clone();
        let split_layers = split_layers_by_tileset(&parts.layers, &parts.tilesets)?;
        let (origin, source_width, source_height) =
            tilemap_bounds(parts.infinite, parts.width, parts.height, &parts.layers);
        let (width, height) = match parts.orientation {
            TilemapOrientation::Isometric => (source_height, source_width),
            _ => (source_width, source_height),
        };
        let mut map = Tilemap::new(TilemapDescriptor::new(
            width,
            height,
            split_layers.len() as u32,
        ));
        let map_pixel_height = source_height as f32 * parts.tile_height as f32;
        let parallax_origin = [
            parts.parallax_origin[0],
            map_pixel_height - parts.parallax_origin[1],
        ];

        let depth_sort = tiled_depth_sort_for_layers(
            parts.orientation,
            [parts.tile_width, parts.tile_height],
            &parts.tilesets,
            split_layers.len(),
        );
        let mut imported_layers = Vec::with_capacity(split_layers.len());
        for (layer_index, split_layer) in split_layers.into_iter().enumerate() {
            let parsed_layer = &parts.layers[split_layer.source_layer];
            let tileset = &parts.tilesets[split_layer.tileset_index];
            for cell in split_layer.cells {
                let Some((tile, x, y)) = decode_cell(
                    cell,
                    tileset,
                    parts.orientation,
                    origin,
                    source_width,
                    source_height,
                )?
                else {
                    continue;
                };
                let _ = map.set_tile(layer_index as u32, x, y, tile);
            }
            imported_layers.push(TiledLayer {
                name: parsed_layer.name.clone(),
                source_layer: split_layer.source_layer,
                tileset_index: split_layer.tileset_index,
                storage_layer: layer_index as u32,
                source_order: parsed_layer.source_order,
                sorting_layer: match depth_sort {
                    TilemapDepthSort::Layer => parsed_layer
                        .source_order
                        .saturating_mul(256)
                        .saturating_add(split_layer.split_order as i32),
                    TilemapDepthSort::YThenLayer => 0,
                },
                visible: parsed_layer.visible,
                opacity: parsed_layer.opacity,
                offset: parsed_layer.offset,
                parallax: parsed_layer.parallax,
                properties: parsed_layer.properties.clone(),
            });
        }
        let mut object_layers = Vec::with_capacity(parts.object_layers.len());
        for parsed_layer in parts.object_layers {
            let mut objects = Vec::with_capacity(parsed_layer.objects.len());
            for object in parsed_layer.objects {
                if let Some(object) = decode_object(object, &parts.tilesets, map_pixel_height)? {
                    objects.push(object);
                }
            }
            object_layers.push(TiledObjectLayer {
                name: parsed_layer.name,
                source_order: parsed_layer.source_order,
                sorting_layer: parsed_layer.source_order,
                visible: parsed_layer.visible,
                opacity: parsed_layer.opacity,
                offset: parsed_layer.offset,
                parallax: parsed_layer.parallax,
                properties: parsed_layer.properties,
                objects,
            });
        }

        Ok(Self {
            map,
            orientation: parts.orientation,
            stagger_axis: parts.stagger_axis,
            stagger_index: parts.stagger_index,
            hex_side_length: parts.hex_side_length,
            render_order: parts.render_order,
            depth_sort,
            tile_size: [parts.tile_width, parts.tile_height],
            tile_origin: [origin.0, origin.1],
            parallax_origin,
            properties: parts.properties,
            tilesets: parts.tilesets,
            tileset: primary_tileset,
            layers: imported_layers,
            object_layers,
        })
    }

    pub(super) fn renderer_stagger_index(&self) -> TilemapStaggerIndex {
        if !matches!(
            self.orientation,
            TilemapOrientation::Staggered | TilemapOrientation::Hexagonal
        ) {
            return self.stagger_index;
        }

        let parity_source = match self.stagger_axis {
            TilemapStaggerAxis::X => self.tile_origin[0],
            TilemapStaggerAxis::Y => {
                self.tile_origin[1].saturating_add(self.map.height().saturating_sub(1) as i32)
            }
        };
        if parity_source & 1 != 0 {
            self.stagger_index.inverted()
        } else {
            self.stagger_index
        }
    }
}

pub(super) struct ParsedMap {
    pub(super) orientation: TilemapOrientation,
    pub(super) stagger_axis: TilemapStaggerAxis,
    pub(super) stagger_index: TilemapStaggerIndex,
    pub(super) hex_side_length: u32,
    pub(super) render_order: TilemapRenderOrder,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) tile_width: u32,
    pub(super) tile_height: u32,
    pub(super) infinite: bool,
    pub(super) parallax_origin: [f32; 2],
    pub(super) properties: Vec<TiledProperty>,
    pub(super) layers: Vec<ParsedLayer>,
    pub(super) object_layers: Vec<ParsedObjectLayer>,
    pub(super) tilesets: Vec<TiledTileset>,
}

pub(super) fn parse_orientation(value: &str) -> Result<TilemapOrientation, TiledImportError> {
    match value {
        "orthogonal" => Ok(TilemapOrientation::Orthogonal),
        "isometric" => Ok(TilemapOrientation::Isometric),
        "staggered" => Ok(TilemapOrientation::Staggered),
        "hexagonal" => Ok(TilemapOrientation::Hexagonal),
        other => Err(TiledImportError::UnsupportedOrientation(other.to_string())),
    }
}

pub(super) fn parse_stagger_axis(value: &str) -> Result<TilemapStaggerAxis, TiledImportError> {
    match value {
        "x" => Ok(TilemapStaggerAxis::X),
        "y" => Ok(TilemapStaggerAxis::Y),
        _ => Err(TiledImportError::UnsupportedLayerData {
            layer: "map".to_string(),
            reason: "unsupported staggeraxis; expected x or y",
        }),
    }
}

pub(super) fn parse_stagger_index(value: &str) -> Result<TilemapStaggerIndex, TiledImportError> {
    match value {
        "odd" => Ok(TilemapStaggerIndex::Odd),
        "even" => Ok(TilemapStaggerIndex::Even),
        _ => Err(TiledImportError::UnsupportedLayerData {
            layer: "map".to_string(),
            reason: "unsupported staggerindex; expected odd or even",
        }),
    }
}

pub(super) fn parse_render_order(value: &str) -> Result<TilemapRenderOrder, TiledImportError> {
    match value {
        "right-down" => Ok(TilemapRenderOrder::RightDown),
        "right-up" => Ok(TilemapRenderOrder::RightUp),
        "left-down" => Ok(TilemapRenderOrder::LeftDown),
        "left-up" => Ok(TilemapRenderOrder::LeftUp),
        other => Err(TiledImportError::UnsupportedLayerData {
            layer: "map".to_string(),
            reason: match other {
                "" => "renderorder is empty",
                _ => {
                    "unsupported renderorder; expected right-down, right-up, left-down, or left-up"
                }
            },
        }),
    }
}

fn collect_tile_layers(
    raw_layers: &[TiledJsonLayer],
    context: LayerContext,
    source_order: &mut i32,
    out: &mut Vec<ParsedLayer>,
    object_layers: &mut Vec<ParsedObjectLayer>,
    base_dir: &Path,
) -> Result<(), TiledImportError> {
    for raw in raw_layers {
        let child_context = LayerContext {
            visible: context.visible && raw.visible,
            opacity: (context.opacity * raw.opacity).clamp(0.0, 1.0),
            offset: [
                context.offset[0] + raw.offsetx,
                context.offset[1] + raw.offsety,
            ],
            parallax: [
                context.parallax[0] * raw.parallaxx,
                context.parallax[1] * raw.parallaxy,
            ],
        };
        match raw.layer_type.as_str() {
            "tilelayer" => {
                let order = *source_order;
                *source_order = source_order.saturating_add(1);
                out.push(ParsedLayer {
                    name: raw.name.clone(),
                    source_order: order,
                    visible: child_context.visible,
                    opacity: child_context.opacity,
                    offset: child_context.offset,
                    parallax: child_context.parallax,
                    properties: collect_json_properties(&raw.properties, base_dir)?,
                    cells: collect_layer_cells(raw)?,
                });
            }
            "objectgroup" => {
                let order = *source_order;
                *source_order = source_order.saturating_add(1);
                let objects = raw
                    .objects
                    .iter()
                    .map(|object| collect_json_object(object, base_dir))
                    .collect::<Result<Vec<_>, _>>()?;
                object_layers.push(ParsedObjectLayer {
                    name: raw.name.clone(),
                    source_order: order,
                    visible: child_context.visible,
                    opacity: child_context.opacity,
                    offset: child_context.offset,
                    parallax: child_context.parallax,
                    properties: collect_json_properties(&raw.properties, base_dir)?,
                    objects,
                });
            }
            "group" => {
                collect_tile_layers(
                    &raw.layers,
                    child_context,
                    source_order,
                    out,
                    object_layers,
                    base_dir,
                )?;
            }
            _ => {
                *source_order = source_order.saturating_add(1);
            }
        }
    }
    Ok(())
}

fn collect_layer_cells(layer: &TiledJsonLayer) -> Result<Vec<RawCell>, TiledImportError> {
    if !layer.chunks.is_empty() {
        let mut cells = Vec::new();
        for chunk in &layer.chunks {
            let data = decode_json_gids(
                &chunk.data,
                &layer.name,
                chunk.encoding.as_deref().or(layer.encoding.as_deref()),
                chunk
                    .compression
                    .as_deref()
                    .or(layer.compression.as_deref()),
            )?;
            let expected = chunk.width as usize * chunk.height as usize;
            if data.len() != expected {
                return Err(TiledImportError::UnsupportedLayerData {
                    layer: layer.name.clone(),
                    reason: "chunk data length does not match width * height",
                });
            }
            for local_y in 0..chunk.height {
                for local_x in 0..chunk.width {
                    let index = (local_y * chunk.width + local_x) as usize;
                    cells.push(RawCell {
                        x: chunk.x + local_x as i32,
                        y: chunk.y + local_y as i32,
                        gid: data[index],
                    });
                }
            }
        }
        return Ok(cells);
    }

    let Some(data) = &layer.data else {
        return Ok(Vec::new());
    };
    let data = decode_json_gids(
        data,
        &layer.name,
        layer.encoding.as_deref(),
        layer.compression.as_deref(),
    )?;
    let width = layer
        .width
        .ok_or_else(|| TiledImportError::UnsupportedLayerData {
            layer: layer.name.clone(),
            reason: "finite tile layer is missing width",
        })?;
    let height = layer
        .height
        .ok_or_else(|| TiledImportError::UnsupportedLayerData {
            layer: layer.name.clone(),
            reason: "finite tile layer is missing height",
        })?;
    let expected = width as usize * height as usize;
    if data.len() != expected {
        return Err(TiledImportError::UnsupportedLayerData {
            layer: layer.name.clone(),
            reason: "tile data length does not match width * height",
        });
    }

    let mut cells = Vec::with_capacity(data.len());
    for local_y in 0..height {
        for local_x in 0..width {
            let index = (local_y * width + local_x) as usize;
            cells.push(RawCell {
                x: layer.x + local_x as i32,
                y: layer.y + local_y as i32,
                gid: data[index],
            });
        }
    }
    Ok(cells)
}
