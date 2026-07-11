use super::*;

fn sample_json(data: &str) -> String {
    format!(
        r#"{{
                "orientation": "orthogonal",
                "width": 3,
                "height": 2,
                "tilewidth": 16,
                "tileheight": 16,
                "layers": [
                    {{
                        "name": "Ground",
                        "type": "tilelayer",
                        "width": 3,
                        "height": 2,
                        "data": {data}
                    }},
                    {{
                        "name": "Foreground",
                        "type": "tilelayer",
                        "width": 3,
                        "height": 2,
                        "opacity": 0.5,
                        "data": [0, 0, 0, 0, 3, 0]
                    }}
                ],
                "tilesets": [
                    {{
                        "firstgid": 1,
                        "image": "tiles.png",
                        "tilewidth": 16,
                        "tileheight": 16,
                        "columns": 2,
                        "tilecount": 4
                    }}
                ]
            }}"#
    )
}

fn write_solid_png(path: &Path, width: u32, height: u32, color: [u8; 4]) {
    let mut image = image::RgbaImage::new(width, height);
    for pixel in image.pixels_mut() {
        *pixel = image::Rgba(color);
    }
    image.save(path).expect("write fixture png");
}

fn write_transparent_png_with_rect(
    path: &Path,
    width: u32,
    height: u32,
    rect: TilesetTileRect,
    color: [u8; 4],
) {
    let mut image = image::RgbaImage::new(width, height);
    for y in rect.y..rect.y + rect.height {
        for x in rect.x..rect.x + rect.width {
            image.put_pixel(x, y, image::Rgba(color));
        }
    }
    image.save(path).expect("write fixture png");
}

fn texture_pixel(texture: &TextureAsset, x: u32, y: u32) -> [u8; 4] {
    let index = ((y * texture.width() + x) * 4) as usize;
    texture.pixels()[index..index + 4]
        .try_into()
        .expect("pixel slice has four channels")
}

#[test]
fn imports_embedded_tiled_json_tile_layers() {
    let import =
        TiledImport::from_json_str(&sample_json("[1, 2, 0, 0, 0, 0]"), Path::new("assets"))
            .expect("Tiled JSON should import");

    assert_eq!(import.orientation, TilemapOrientation::Orthogonal);
    assert_eq!(import.render_order, TilemapRenderOrder::RightDown);
    assert_eq!(import.tile_size, [16, 16]);
    assert_eq!(
        import.tileset.image,
        PathBuf::from("assets").join("tiles.png")
    );
    assert_eq!(import.tileset.columns, 2);
    assert_eq!(import.layers.len(), 2);
    assert_eq!(import.layers[0].name, "Ground");
    assert_eq!(import.layers[0].sorting_layer, 0);
    assert_eq!(import.layers[1].sorting_layer, 256);
    assert_eq!(import.layers[1].opacity, 0.5);
    assert_eq!(import.map.tile(0, 0, 1).unwrap().id, TileId(0));
    assert_eq!(import.map.tile(0, 1, 1).unwrap().id, TileId(1));
    assert_eq!(import.map.tile(1, 1, 0).unwrap().id, TileId(2));
}

#[test]
fn imports_tiled_flip_flags() {
    let flipped = FLIPPED_HORIZONTALLY_FLAG | FLIPPED_VERTICALLY_FLAG | 2;
    let import = TiledImport::from_json_str(
        &sample_json(&format!("[{flipped}, 0, 0, 0, 0, 0]")),
        Path::new("."),
    )
    .expect("Tiled JSON should import");

    let tile = import.map.tile(0, 0, 1).unwrap();
    assert_eq!(tile.id, TileId(1));
    assert!(tile.flags.contains(TileFlags::FLIP_X));
    assert!(tile.flags.contains(TileFlags::FLIP_Y));
}

#[test]
fn imports_diagonal_tiled_flip() {
    let diagonal = FLIPPED_DIAGONALLY_FLAG | 1;
    let import = TiledImport::from_json_str(
        &sample_json(&format!("[{diagonal}, 0, 0, 0, 0, 0]")),
        Path::new("."),
    )
    .expect("diagonal flags should import");

    let tile = import.map.tile(0, 0, 1).unwrap();
    assert_eq!(tile.id, TileId(0));
    assert!(tile.flags.contains(TileFlags::FLIP_DIAGONAL));
}

#[test]
fn imports_multiple_used_tilesets_as_split_layers() {
    let json = r#"{
            "orientation": "orthogonal",
            "width": 2,
            "height": 1,
            "tilewidth": 16,
            "tileheight": 16,
            "layers": [{
                "name": "Ground",
                "type": "tilelayer",
                "width": 2,
                "height": 1,
                "data": [1, 10]
            }],
            "tilesets": [
                {
                    "firstgid": 1,
                    "image": "a.png",
                    "tilewidth": 16,
                    "tileheight": 16,
                    "columns": 1,
                    "tilecount": 1
                },
                {
                    "firstgid": 10,
                    "image": "b.png",
                    "tilewidth": 16,
                    "tileheight": 16,
                    "columns": 1,
                    "tilecount": 1
                }
            ]
        }"#;
    let import =
        TiledImport::from_json_str(json, Path::new(".")).expect("multiple tilesets import");

    assert_eq!(import.tilesets.len(), 2);
    assert_eq!(import.layers.len(), 2);
    assert_eq!(import.layers[0].source_layer, 0);
    assert_eq!(import.layers[0].tileset_index, 0);
    assert_eq!(import.layers[0].storage_layer, 0);
    assert_eq!(import.layers[1].source_layer, 0);
    assert_eq!(import.layers[1].tileset_index, 1);
    assert_eq!(import.layers[1].storage_layer, 1);
    assert_eq!(import.map.tile(0, 0, 0).unwrap().id, TileId(0));
    assert_eq!(import.map.tile(1, 1, 0).unwrap().id, TileId(0));

    let texture_a = Handle::<TextureAsset>::new(crate::asset::AssetId::new());
    let texture_b = Handle::<TextureAsset>::new(crate::asset::AssetId::new());
    let renderer_a = import
        .renderer_for_layer(
            TilemapHandle::new(0, 0),
            &[texture_a.clone(), texture_b.clone()],
            0,
        )
        .expect("first split layer should render");
    let renderer_b = import
        .renderer_for_layer(
            TilemapHandle::new(0, 0),
            &[texture_a.clone(), texture_b.clone()],
            1,
        )
        .expect("second split layer should render");
    assert_eq!(renderer_a.tileset.texture, texture_a);
    assert_eq!(renderer_b.tileset.texture, texture_b);
}

#[test]
fn imports_official_object_shapes_and_properties() {
    let import =
        TiledImport::from_file("examples/assets/tiled/tiled/examples/orthogonal-outside.tmx")
            .expect("official orthogonal map should import");

    assert!(import.properties.iter().any(|property| {
        property.name == "enemyTint"
            && matches!(property.value, TiledPropertyValue::Color(color)
                    if (color.r - 0.6392157).abs() < 0.001
                        && (color.a - 1.0).abs() < 0.001)
    }));

    let objects = &import
        .object_layers
        .iter()
        .find(|layer| layer.name == "Objects")
        .expect("Objects layer should import")
        .objects;
    assert!(objects.iter().any(|object| {
        object.name == "maggots"
            && object.class == "Location"
            && matches!(object.shape, TiledObjectShape::Rectangle)
            && object.properties.iter().any(|property| {
                property.name == "spawncount"
                    && matches!(property.value, TiledPropertyValue::Int(5))
            })
    }));
    assert!(objects.iter().any(|object| {
        object.name == "discover chest"
            && object.class == "Trigger"
            && matches!(object.shape, TiledObjectShape::Ellipse)
            && object.properties.iter().any(|property| {
                property.name == "script"
                    && matches!(property.value, TiledPropertyValue::File(ref path)
                            if path.ends_with("chest-discovered.lua"))
            })
    }));
    assert!(objects.iter().any(|object| {
        object.name == "unreachable"
            && object.class == "Fixture"
            && matches!(object.shape, TiledObjectShape::Polygon(ref points) if points.len() > 4)
    }));
    assert!(objects.iter().any(|object| {
        object.name == "guard"
            && object.class == "NPC"
            && matches!(object.shape, TiledObjectShape::Polyline(ref points) if points.len() > 2)
    }));
    assert!(objects.iter().any(|object| {
        object.name == "player-start" && matches!(object.shape, TiledObjectShape::Point)
    }));
    assert!(objects.iter().any(|object| {
        object.class == "Sign"
            && matches!(
                object.shape,
                TiledObjectShape::Tile {
                    tileset_index: 0,
                    ..
                }
            )
    }));
}

#[test]
fn imports_json_object_layers_and_properties() {
    let json = r##"{
            "orientation": "orthogonal",
            "width": 4,
            "height": 4,
            "tilewidth": 16,
            "tileheight": 16,
            "layers": [{
                "name": "Objects",
                "type": "objectgroup",
                "objects": [
                    {
                        "id": 1,
                        "name": "spawn",
                        "class": "Location",
                        "x": 16,
                        "y": 16,
                        "point": true,
                        "properties": [
                            { "name": "enabled", "type": "bool", "value": true },
                            { "name": "weight", "type": "float", "value": 1.5 }
                        ]
                    },
                    {
                        "id": 2,
                        "type": "Trigger",
                        "x": 32,
                        "y": 32,
                        "width": 16,
                        "height": 16,
                        "ellipse": true,
                        "properties": [
                            { "name": "target", "type": "object", "value": 1 },
                            { "name": "tint", "type": "color", "value": "#80ff0000" }
                        ]
                    }
                ]
            }],
            "tilesets": [{
                "firstgid": 1,
                "image": "tiles.png",
                "tilewidth": 16,
                "tileheight": 16,
                "columns": 1,
                "tilecount": 1
            }]
        }"##;
    let import =
        TiledImport::from_json_str(json, Path::new("assets")).expect("JSON objects import");

    let objects = &import.object_layers[0].objects;
    assert_eq!(objects[0].class, "Location");
    assert!(matches!(objects[0].shape, TiledObjectShape::Point));
    assert!(objects[0].properties.iter().any(|property| {
        property.name == "enabled" && matches!(property.value, TiledPropertyValue::Bool(true))
    }));
    assert_eq!(objects[1].class, "Trigger");
    assert!(matches!(objects[1].shape, TiledObjectShape::Ellipse));
    assert!(objects[1].properties.iter().any(|property| {
        property.name == "target" && matches!(property.value, TiledPropertyValue::Object(1))
    }));
}

#[test]
fn imports_external_tsj_tileset() {
    let temp = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        temp.path().join("terrain.tsj"),
        r#"{
                "image": "terrain.png",
                "tilewidth": 8,
                "tileheight": 8,
                "columns": 1,
                "tilecount": 1
            }"#,
    )
    .expect("write tsj");
    let json = r#"{
            "orientation": "orthogonal",
            "width": 1,
            "height": 1,
            "tilewidth": 8,
            "tileheight": 8,
            "layers": [{
                "name": "Ground",
                "type": "tilelayer",
                "width": 1,
                "height": 1,
                "data": [1]
            }],
            "tilesets": [{ "firstgid": 1, "source": "terrain.tsj" }]
        }"#;

    let import = TiledImport::from_json_str(json, temp.path()).expect("external TSJ should import");

    assert_eq!(import.tileset.image, temp.path().join("terrain.png"));
    assert_eq!(import.map.tile(0, 0, 0).unwrap().id, TileId(0));
}

#[test]
fn imports_external_tsx_tile_animations() {
    let temp = tempfile::tempdir().expect("tempdir");
    std::fs::write(
            temp.path().join("water.tsx"),
            r#"<?xml version="1.0" encoding="UTF-8"?>
            <tileset version="1.8" tiledversion="1.8.2" name="water" tilewidth="8" tileheight="8" tilecount="4" columns="4">
                <image source="water.png" width="32" height="8"/>
                <tile id="1">
                    <animation>
                        <frame tileid="1" duration="100"/>
                        <frame tileid="2" duration="150"/>
                    </animation>
                </tile>
            </tileset>"#,
        )
        .expect("write tsx");
    let tmx = r#"<?xml version="1.0" encoding="UTF-8"?>
        <map version="1.8" tiledversion="1.8.2" orientation="orthogonal" width="1" height="1" tilewidth="8" tileheight="8">
            <tileset firstgid="1" source="water.tsx"/>
            <layer name="Ground" width="1" height="1">
                <data><tile gid="2"/></data>
            </layer>
        </map>"#;

    let import = TiledImport::from_tmx_str(tmx, temp.path()).expect("external TSX should import");

    assert_eq!(import.tileset.animations.len(), 1);
    assert_eq!(import.tileset.animations[0].tile_id, TileId(1));
    assert_eq!(import.tileset.animations[0].frame_at(0.12), TileId(2));
    assert_eq!(import.map.tile(0, 0, 0).unwrap().id, TileId(1));
}

#[test]
fn imports_tsx_image_dimensions_and_tile_offset() {
    let temp = tempfile::tempdir().expect("tempdir");
    image::RgbaImage::new(128, 64)
        .save(temp.path().join("walls.png"))
        .expect("write png");
    std::fs::write(
        temp.path().join("walls.tsx"),
        r#"<?xml version="1.0" encoding="UTF-8"?>
            <tileset name="walls" tilewidth="64" tileheight="64">
                <tileoffset x="-32" y="4"/>
                <image source="walls.png"/>
            </tileset>"#,
    )
    .expect("write tsx");
    let tmx = r#"<?xml version="1.0" encoding="UTF-8"?>
        <map version="1.0" orientation="orthogonal" width="1" height="1" tilewidth="31" tileheight="31">
            <tileset firstgid="1" source="walls.tsx"/>
            <layer name="Walls" width="1" height="1">
                <data><tile gid="1"/></data>
            </layer>
        </map>"#;

    let import = TiledImport::from_tmx_str(tmx, temp.path()).expect("external TSX should import");
    let texture = Handle::<TextureAsset>::new(crate::asset::AssetId::new());
    let renderer = import
        .renderer_for_layer(TilemapHandle::new(0, 0), &[texture], 0)
        .expect("layer renderer");

    assert_eq!(import.tile_size, [31, 31]);
    assert_eq!(import.tileset.tile_size, [64, 64]);
    assert_eq!(import.tileset.image_size, [128, 64]);
    assert_eq!(import.tileset.columns, 2);
    assert_eq!(import.tileset.tile_count, 2);
    assert_eq!(import.tileset.tile_offset, [-32, 4]);
    assert_eq!(import.tileset.margin, 0);
    assert_eq!(import.tileset.spacing, 0);
    assert_eq!(renderer.tile_size, [31.0, 31.0]);
    assert_eq!(renderer.tile_draw_size, [64.0, 64.0]);
    assert_eq!(renderer.tile_offset, [-32.0, -4.0]);
}

#[test]
fn imports_tmx_tileset_margin_and_spacing() {
    let import = TiledImport::from_tmx_file("examples/assets/tiled/tiled/examples/desert.tmx")
        .expect("Tiled spacing/margin sample should import");
    let texture = Handle::<TextureAsset>::new(crate::asset::AssetId::new());
    let grid = import.tileset_grid(texture);

    assert_eq!(import.tileset.margin, 1);
    assert_eq!(import.tileset.spacing, 1);
    assert_eq!(import.tileset.columns, 8);
    assert_eq!(import.tileset.rows, 6);
    assert_eq!(import.tileset.tile_count, 48);
    assert_eq!(grid.texture_size, [265, 199]);
    for (actual, expected) in grid.uv_rect(TileId(9)).unwrap().into_iter().zip([
        34.0 / 265.0,
        34.0 / 199.0,
        66.0 / 265.0,
        66.0 / 199.0,
    ]) {
        assert!((actual - expected).abs() <= 1e-6);
    }
}

#[test]
fn imports_official_isometric_tmx_layer_with_tiled_direction() {
    let import = TiledImport::from_tmx_file(
        "examples/assets/tiled/tiled/examples/isometric_grass_and_water.tmx",
    )
    .expect("official Tiled isometric TMX sample should import");

    assert_eq!(import.orientation, TilemapOrientation::Isometric);
    assert_eq!(import.render_order, TilemapRenderOrder::RightDown);
    assert_eq!(import.tile_size, [64, 32]);
    assert_eq!(import.map.width(), 25);
    assert_eq!(import.map.height(), 25);
    assert_eq!(import.layers.len(), 1);
    assert_eq!(import.tileset.tile_size, [64, 64]);
    assert_eq!(import.tileset.tile_offset, [0, 16]);

    let texture = Handle::<TextureAsset>::new(crate::asset::AssetId::new());
    let renderer = import
        .renderer_for_layer(TilemapHandle::new(0, 0), &[texture], 0)
        .expect("layer renderer");
    assert_eq!(renderer.tile_offset, [0.0, -16.0]);

    assert_eq!(import.map.tile(0, 24, 24).unwrap().id, TileId(23));
    assert_eq!(import.map.tile(0, 24, 0).unwrap().id, TileId(0));
    assert_eq!(import.map.tile(0, 0, 24).unwrap().id, TileId(0));
    assert_eq!(import.map.tile(0, 0, 0).unwrap().id, TileId(0));
}

#[test]
fn imports_official_forest_image_collection_and_tile_objects() {
    let import =
        TiledImport::from_tmx_file("examples/assets/tiled/tiled/examples/forest/forest.tmx")
            .expect("official Tiled forest TMX sample should import");

    assert_eq!(import.orientation, TilemapOrientation::Orthogonal);
    assert_eq!(import.tile_size, [16, 16]);
    assert_eq!(import.parallax_origin, [320.0, 128.0]);
    assert_eq!(import.tileset.tile_size, [160, 208]);
    assert_eq!(import.tileset.tile_count, 14);
    assert_eq!(
        import.tileset.tile_rects[0],
        Some(TilesetTileRect::new(1, 1, 16, 16))
    );
    assert_eq!(
        import.tileset.tile_rects[13],
        Some(TilesetTileRect::new(116, 824, 25, 25))
    );
    assert_eq!(import.layers.len(), 1);
    assert_eq!(import.layers[0].parallax, [1.0, 1.0]);
    assert_eq!(import.object_layers.len(), 4);
    assert_eq!(import.object_layers[0].objects.len(), 4);
    assert_eq!(import.object_layers[0].parallax, [0.12, 0.12]);
    assert_eq!(import.object_layers[1].parallax, [0.25, 0.25]);
    assert_eq!(import.object_layers[2].parallax, [0.5, 0.5]);
    assert_eq!(import.object_layers[3].parallax, [1.0, 1.0]);
    assert_eq!(
        import.object_layers[3].objects[0].tile_id(),
        Some(TileId(13))
    );

    let texture = Handle::<TextureAsset>::new(crate::asset::AssetId::new());
    let grid = import.tileset_grid(texture);
    let uv = grid.uv_rect(TileId(13)).expect("animated tile source uv");
    assert_eq!(grid.tile_draw_size(TileId(0)), Some([16, 16]));
    assert_eq!(grid.tile_draw_size(TileId(13)), Some([25, 25]));
    for (actual, expected) in uv.into_iter().zip([
        116.0 / 1024.0,
        824.0 / 1024.0,
        141.0 / 1024.0,
        849.0 / 1024.0,
    ]) {
        assert!((actual - expected).abs() <= 1e-6);
    }
}

#[test]
fn packs_tmx_image_collection_tileset_with_multiple_source_images() {
    let temp = tempfile::tempdir().expect("tempdir");
    write_solid_png(&temp.path().join("red.png"), 2, 2, [255, 0, 0, 255]);
    write_solid_png(&temp.path().join("green.png"), 3, 1, [0, 255, 0, 255]);
    std::fs::write(
        temp.path().join("collection.tsx"),
        r#"<?xml version="1.0" encoding="UTF-8"?>
            <tileset name="collection" tilewidth="3" tileheight="2" tilecount="2" columns="0">
                <tile id="0" width="2" height="2">
                    <image width="2" height="2" source="red.png"/>
                </tile>
                <tile id="1" width="3" height="1">
                    <image width="3" height="1" source="green.png"/>
                </tile>
            </tileset>"#,
    )
    .expect("write tsx");
    let tmx = r#"<?xml version="1.0" encoding="UTF-8"?>
        <map version="1.10" orientation="orthogonal" renderorder="right-down" width="2" height="1" tilewidth="3" tileheight="2">
            <tileset firstgid="1" source="collection.tsx"/>
            <layer name="Ground" width="2" height="1">
                <data><tile gid="1"/><tile gid="2"/></data>
            </layer>
        </map>"#;

    let import = TiledImport::from_tmx_str(tmx, temp.path())
        .expect("multi-image image collection should import");

    assert_eq!(import.tileset.image_size, [6, 2]);
    assert_eq!(import.tileset.tile_rects.len(), 2);
    assert_eq!(
        import.tileset.tile_rects[0],
        Some(TilesetTileRect::new(0, 0, 2, 2))
    );
    assert_eq!(
        import.tileset.tile_rects[1],
        Some(TilesetTileRect::new(3, 0, 3, 1))
    );
    assert_eq!(import.tileset.tile_images.len(), 2);

    let texture = import
        .load_tileset_texture()
        .expect("multi-image collection atlas should load");
    assert_eq!(texture.size(), [6, 2]);
    assert_eq!(texture_pixel(&texture, 0, 0), [255, 0, 0, 255]);
    assert_eq!(texture_pixel(&texture, 2, 0), [0, 0, 0, 0]);
    assert_eq!(texture_pixel(&texture, 3, 0), [0, 255, 0, 255]);

    let grid = import.tileset_grid(Handle::<TextureAsset>::new(crate::asset::AssetId::new()));
    assert_eq!(grid.tile_draw_size(TileId(0)), Some([2, 2]));
    assert_eq!(grid.tile_draw_size(TileId(1)), Some([3, 1]));
    for (actual, expected) in grid
        .uv_rect(TileId(1))
        .expect("second tile uv")
        .into_iter()
        .zip([0.5, 0.0, 1.0, 0.5])
    {
        assert!((actual - expected).abs() <= 1e-6);
    }
}

#[test]
fn imports_inline_json_image_collection_tileset_with_multiple_source_images() {
    let temp = tempfile::tempdir().expect("tempdir");
    write_solid_png(&temp.path().join("red.png"), 2, 2, [255, 0, 0, 255]);
    write_solid_png(&temp.path().join("green.png"), 3, 1, [0, 255, 0, 255]);
    let json = r#"{
            "orientation": "orthogonal",
            "renderorder": "right-down",
            "width": 2,
            "height": 1,
            "tilewidth": 3,
            "tileheight": 2,
            "layers": [{
                "name": "Ground",
                "type": "tilelayer",
                "width": 2,
                "height": 1,
                "data": [1, 2]
            }],
            "tilesets": [{
                "firstgid": 1,
                "name": "collection",
                "tilewidth": 3,
                "tileheight": 2,
                "columns": 2,
                "tilecount": 2,
                "tiles": [{
                    "id": 0,
                    "image": "red.png",
                    "imagewidth": 2,
                    "imageheight": 2,
                    "width": 2,
                    "height": 2
                }, {
                    "id": 1,
                    "image": "green.png",
                    "imagewidth": 3,
                    "imageheight": 1,
                    "width": 3,
                    "height": 1
                }]
            }]
        }"#;

    let import =
        TiledImport::from_json_str(json, temp.path()).expect("inline image collection import");

    assert_eq!(import.map.tile(0, 0, 0).unwrap().id, TileId(0));
    assert_eq!(import.map.tile(0, 1, 0).unwrap().id, TileId(1));
    assert_eq!(import.tileset.image_size, [6, 2]);
    assert_eq!(
        import.tileset.tile_rects[0],
        Some(TilesetTileRect::new(0, 0, 2, 2))
    );
    assert_eq!(
        import.tileset.tile_rects[1],
        Some(TilesetTileRect::new(3, 0, 3, 1))
    );
    assert_eq!(import.tileset.tile_images.len(), 2);

    let texture = import
        .load_tileset_texture()
        .expect("inline image collection atlas should load");
    assert_eq!(texture.size(), [6, 2]);
    assert_eq!(texture_pixel(&texture, 0, 0), [255, 0, 0, 255]);
    assert_eq!(texture_pixel(&texture, 2, 0), [0, 0, 0, 0]);
    assert_eq!(texture_pixel(&texture, 3, 0), [0, 255, 0, 255]);
}

#[test]
fn imports_isometric_large_tile_over_small_cell_without_trimming() {
    let temp = tempfile::tempdir().expect("tempdir");
    write_transparent_png_with_rect(
        &temp.path().join("tower.png"),
        256,
        512,
        TilesetTileRect::new(96, 320, 64, 128),
        [80, 180, 255, 255],
    );
    std::fs::write(
        temp.path().join("large.tsx"),
        r#"<?xml version="1.0" encoding="UTF-8"?>
            <tileset name="large" tilewidth="256" tileheight="512" tilecount="1" columns="0">
                <tile id="0" width="256" height="512">
                    <image width="256" height="512" source="tower.png"/>
                </tile>
            </tileset>"#,
    )
    .expect("write tsx");
    let tmx = r#"<?xml version="1.0" encoding="UTF-8"?>
        <map version="1.10" orientation="isometric" renderorder="right-down" width="1" height="1" tilewidth="256" tileheight="128">
            <tileset firstgid="1" source="large.tsx"/>
            <layer name="Ground" width="1" height="1">
                <data><tile gid="1"/></data>
            </layer>
        </map>"#;

    let import = TiledImport::from_tmx_str(tmx, temp.path())
        .expect("large isometric tile fixture should import");
    assert_eq!(import.tile_size, [256, 128]);
    assert_eq!(import.tileset.tile_size, [256, 512]);
    assert_eq!(import.tileset.tile_draw_size(TileId(0)), [256, 512]);

    let texture = import
        .load_tileset_texture()
        .expect("large transparent tile texture should load");
    assert_eq!(texture.size(), [256, 512]);
    assert_eq!(texture_pixel(&texture, 0, 0), [0, 0, 0, 0]);
    assert_eq!(texture_pixel(&texture, 96, 320), [80, 180, 255, 255]);

    let renderer = import
        .renderer_for_layer(
            TilemapHandle::new(0, 0),
            &[Handle::<TextureAsset>::new(crate::asset::AssetId::new())],
            0,
        )
        .expect("layer renderer");
    assert_eq!(renderer.tile_size, [256.0, 128.0]);
    assert_eq!(renderer.tile_draw_size, [256.0, 512.0]);
    assert_eq!(renderer.cell_to_local_origin([0, 0]), [-128.0, -64.0]);
}

#[test]
fn imports_official_staggered_tmx_layer() {
    let import = TiledImport::from_tmx_file(
        "examples/assets/tiled/tiled/examples/isometric_staggered_grass_and_water.tmx",
    )
    .expect("official Tiled staggered TMX sample should import");

    assert_eq!(import.orientation, TilemapOrientation::Staggered);
    assert_eq!(import.stagger_axis, TilemapStaggerAxis::Y);
    assert_eq!(import.stagger_index, TilemapStaggerIndex::Odd);
    assert_eq!(import.render_order, TilemapRenderOrder::RightDown);
    assert_eq!(import.tile_size, [64, 32]);
    assert_eq!(import.map.width(), 32);
    assert_eq!(import.map.height(), 64);
    assert_eq!(import.layers.len(), 1);
    assert_eq!(import.tileset.tile_size, [64, 64]);
    assert_eq!(import.tileset.tile_offset, [0, 16]);

    let texture = Handle::<TextureAsset>::new(crate::asset::AssetId::new());
    let renderer = import
        .renderer_for_layer(TilemapHandle::new(0, 0), &[texture], 0)
        .expect("layer renderer");
    assert_eq!(renderer.stagger_axis, TilemapStaggerAxis::Y);
    assert_eq!(renderer.stagger_index, TilemapStaggerIndex::Even);
    assert_eq!(renderer.tile_offset, [0.0, -16.0]);
    assert_eq!(renderer.cell_to_local_origin([0, 0]), [32.0, 0.0]);
    assert_eq!(renderer.cell_to_local_origin([0, 63]), [0.0, 1008.0]);

    let mut non_empty = 0usize;
    for y in 0..import.map.height() {
        for x in 0..import.map.width() {
            if !import.map.tile(0, x, y).unwrap().is_empty() {
                non_empty += 1;
            }
        }
    }
    assert!(non_empty > 0);
}

#[test]
fn imports_official_hexagonal_tmx_layer() {
    let import =
        TiledImport::from_tmx_file("examples/assets/tiled/tiled/examples/hexagonal-mini.tmx")
            .expect("official Tiled hexagonal TMX sample should import");

    assert_eq!(import.orientation, TilemapOrientation::Hexagonal);
    assert_eq!(import.stagger_axis, TilemapStaggerAxis::Y);
    assert_eq!(import.stagger_index, TilemapStaggerIndex::Odd);
    assert_eq!(import.hex_side_length, 6);
    assert_eq!(import.tile_size, [14, 12]);
    assert_eq!(import.map.width(), 20);
    assert_eq!(import.map.height(), 20);
    assert_eq!(import.layers.len(), 1);
    assert_eq!(import.tileset.tile_size, [18, 18]);
    assert_eq!(import.tileset.tile_offset, [0, 1]);
    assert_eq!(import.tileset.columns, 5);
    assert_eq!(import.tileset.rows, 4);

    let texture = Handle::<TextureAsset>::new(crate::asset::AssetId::new());
    let renderer = import
        .renderer_for_layer(TilemapHandle::new(0, 0), &[texture], 0)
        .expect("layer renderer");
    assert_eq!(renderer.hex_side_length, 6.0);
    assert_eq!(renderer.stagger_index, TilemapStaggerIndex::Even);
    assert_eq!(renderer.cell_to_local_origin([0, 0]), [7.0, 0.0]);
    assert_eq!(renderer.cell_to_local_origin([0, 1]), [0.0, 9.0]);

    let mut non_empty = 0usize;
    for y in 0..import.map.height() {
        for x in 0..import.map.width() {
            if !import.map.tile(0, x, y).unwrap().is_empty() {
                non_empty += 1;
            }
        }
    }
    assert!(non_empty > 0);
}

#[test]
fn tmx_overhanging_multilayer_tiles_enable_y_then_layer_sort() {
    let temp = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        temp.path().join("walls.tsx"),
        r#"<?xml version="1.0" encoding="UTF-8"?>
            <tileset name="walls" tilewidth="64" tileheight="64" columns="1" tilecount="1">
                <tileoffset x="-32" y="0"/>
                <image source="walls.png" width="64" height="64"/>
            </tileset>"#,
    )
    .expect("write tsx");
    let tmx = r#"<?xml version="1.0" encoding="UTF-8"?>
        <map version="1.0" orientation="orthogonal" renderorder="right-down" width="2" height="2" tilewidth="31" tileheight="31">
            <tileset firstgid="1" source="walls.tsx"/>
            <layer name="Walls" width="2" height="2">
                <data><tile gid="1"/><tile gid="0"/><tile gid="0"/><tile gid="0"/></data>
            </layer>
            <layer name="Walls level 2" width="2" height="2">
                <data><tile gid="0"/><tile gid="0"/><tile gid="0"/><tile gid="1"/></data>
            </layer>
        </map>"#;

    let import =
        TiledImport::from_tmx_str(tmx, temp.path()).expect("overhanging layers should import");
    let texture = Handle::<TextureAsset>::new(crate::asset::AssetId::new());
    let renderer = import
        .renderer_for_layer(TilemapHandle::new(0, 0), &[texture], 0)
        .expect("layer renderer");

    assert_eq!(import.depth_sort, TilemapDepthSort::YThenLayer);
    assert_eq!(import.layers[0].sorting_layer, 0);
    assert_eq!(import.layers[1].sorting_layer, 0);
    assert_eq!(renderer.depth_sort, TilemapDepthSort::YThenLayer);
}

#[test]
fn imports_official_tmx_base64_zlib_layers() {
    let import = TiledImport::from_tmx_file("examples/assets/tiled/sewers.tmx")
        .expect("official Tiled TMX sample should import");

    assert_eq!(import.orientation, TilemapOrientation::Orthogonal);
    assert_eq!(import.render_order, TilemapRenderOrder::RightDown);
    assert_eq!(import.tile_size, [24, 24]);
    assert_eq!(import.map.width(), 50);
    assert_eq!(import.map.height(), 50);
    assert_eq!(import.layers.len(), 2);
    assert_eq!(import.layers[0].name, "Bottom");
    assert_eq!(import.layers[1].name, "Top");
    assert_eq!(import.layers[1].opacity, 0.49);
    assert_eq!(import.tileset.image_size, [192, 217]);
    assert_eq!(import.tileset.transparent_color, Some([255, 0, 255]));

    let mut non_empty = 0usize;
    for layer in 0..import.map.layer_count() {
        for y in 0..import.map.height() {
            for x in 0..import.map.width() {
                if !import.map.tile(layer, x, y).unwrap().is_empty() {
                    non_empty += 1;
                }
            }
        }
    }
    assert!(non_empty > 0);
}
