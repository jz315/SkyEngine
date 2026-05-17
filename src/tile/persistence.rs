use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::render::{Color, TileFlags, TilemapRenderOrder};

use super::document::{TileAuthoringFormat, TileAuthoringMetadata, TileMapDocument};
use super::grid::{
    CellCoord, CellRect, GridOrientation, GridOrigin, GridSpec, StaggerAxis, StaggerIndex,
    TileDirection,
};
use super::layer::{
    CollisionLayerData, LayerData, LayerId, LayerKind, LayerRole, MetadataLayerData,
    ObjectLayerData, SceneTile, TileLayer, TileLayerData, TileRef,
};
use super::object::{
    Footprint, ObjectPrototypeId, ObjectVisual, ObjectVisualTile, SpriteVisualRef, TileObject,
    TileObjectId,
};
use super::palette::{
    AssetSource, PaletteId, PropertyBag, PropertyValue, RectU, TileAnimation, TileAnimationFrame,
    TileAtlasImageSource, TileCollision, TileDef, TileDefId, TilePalette, TilePaletteStore,
    TileTextureSource,
};
use super::scene::{TileMap, TileMapId, TileMapSize};
use super::{
    DirtyCell, DirtyRegion, ObjectChange, PropertyChange, PropertyTarget, TileChange,
    TileMapEditSession, TileMapEditSummary,
};

const TILE_SNAPSHOT_VERSION: u32 = 1;
const TILE_DELTA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TileDocumentRevision(pub u64);

impl TileDocumentRevision {
    pub const fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

#[derive(Debug)]
pub enum TilePersistenceError {
    Json(serde_json::Error),
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    UnsupportedVersion {
        kind: &'static str,
        version: u32,
    },
    RuntimeTextureSource {
        palette: PaletteId,
    },
    SceneMismatch {
        document: TileMapId,
        delta: TileMapId,
    },
    FutureDelta {
        base_revision: TileDocumentRevision,
        current_revision: TileDocumentRevision,
    },
}

impl fmt::Display for TilePersistenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(source) => write!(f, "tile persistence JSON error: {source}"),
            Self::Io { path, source } => {
                write!(
                    f,
                    "tile persistence I/O error at {}: {source}",
                    path.display()
                )
            }
            Self::UnsupportedVersion { kind, version } => {
                write!(f, "unsupported tile {kind} version {version}")
            }
            Self::RuntimeTextureSource { palette } => write!(
                f,
                "palette {:?} uses a runtime texture handle and cannot be persisted",
                palette
            ),
            Self::SceneMismatch { document, delta } => write!(
                f,
                "tile delta scene {:?} does not match document scene {:?}",
                delta, document
            ),
            Self::FutureDelta {
                base_revision,
                current_revision,
            } => write!(
                f,
                "tile delta base revision {:?} is newer than current document revision {:?}",
                base_revision, current_revision
            ),
        }
    }
}

impl std::error::Error for TilePersistenceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Json(source) => Some(source),
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<serde_json::Error> for TilePersistenceError {
    fn from(source: serde_json::Error) -> Self {
        Self::Json(source)
    }
}

impl From<std::io::Error> for TilePersistenceError {
    fn from(source: std::io::Error) -> Self {
        Self::Io {
            path: PathBuf::new(),
            source,
        }
    }
}

#[derive(Clone, Debug)]
pub struct TileMapSnapshot {
    data: SnapshotDto,
}

impl TileMapSnapshot {
    pub fn from_document(document: &TileMapDocument) -> Result<Self, TilePersistenceError> {
        Ok(Self {
            data: SnapshotDto::from_document(document)?,
        })
    }

    pub fn into_document(self) -> Result<TileMapDocument, TilePersistenceError> {
        self.data.into_document()
    }

    pub fn from_json_str(input: &str) -> Result<Self, TilePersistenceError> {
        let data = serde_json::from_str::<SnapshotDto>(input)?;
        if data.version != TILE_SNAPSHOT_VERSION {
            return Err(TilePersistenceError::UnsupportedVersion {
                kind: "snapshot",
                version: data.version,
            });
        }
        Ok(Self { data })
    }

    pub fn from_json_file(path: impl AsRef<Path>) -> Result<Self, TilePersistenceError> {
        let path = path.as_ref();
        Self::from_json_str(&fs::read_to_string(path).map_err(|source| {
            TilePersistenceError::Io {
                path: path.to_path_buf(),
                source,
            }
        })?)
    }

    pub fn to_json_string(&self) -> Result<String, TilePersistenceError> {
        serde_json::to_string(&self.data).map_err(TilePersistenceError::Json)
    }

    pub fn to_json_string_pretty(&self) -> Result<String, TilePersistenceError> {
        serde_json::to_string_pretty(&self.data).map_err(TilePersistenceError::Json)
    }

    pub fn write_json_file(&self, path: impl AsRef<Path>) -> Result<(), TilePersistenceError> {
        let path = path.as_ref();
        fs::write(path, self.to_json_string_pretty()?).map_err(|source| TilePersistenceError::Io {
            path: path.to_path_buf(),
            source,
        })
    }
}

#[derive(Clone, Debug)]
pub struct TileMapDelta {
    pub base_scene: TileMapId,
    pub base_revision: TileDocumentRevision,
    pub changes: TileMapEditSummary,
}

impl TileMapDelta {
    pub fn new(
        base_scene: TileMapId,
        base_revision: TileDocumentRevision,
        changes: TileMapEditSummary,
    ) -> Self {
        Self {
            base_scene,
            base_revision,
            changes,
        }
    }

    pub fn from_json_str(input: &str) -> Result<Self, TilePersistenceError> {
        let data = serde_json::from_str::<DeltaDto>(input)?;
        if data.version != TILE_DELTA_VERSION {
            return Err(TilePersistenceError::UnsupportedVersion {
                kind: "delta",
                version: data.version,
            });
        }
        Ok(data.into_delta())
    }

    pub fn from_json_file(path: impl AsRef<Path>) -> Result<Self, TilePersistenceError> {
        let path = path.as_ref();
        Self::from_json_str(&fs::read_to_string(path).map_err(|source| {
            TilePersistenceError::Io {
                path: path.to_path_buf(),
                source,
            }
        })?)
    }

    pub fn to_json_string(&self) -> Result<String, TilePersistenceError> {
        serde_json::to_string(&DeltaDto::from_delta(self)).map_err(TilePersistenceError::Json)
    }

    pub fn to_json_string_pretty(&self) -> Result<String, TilePersistenceError> {
        serde_json::to_string_pretty(&DeltaDto::from_delta(self))
            .map_err(TilePersistenceError::Json)
    }

    pub fn write_json_file(&self, path: impl AsRef<Path>) -> Result<(), TilePersistenceError> {
        let path = path.as_ref();
        fs::write(path, self.to_json_string_pretty()?).map_err(|source| TilePersistenceError::Io {
            path: path.to_path_buf(),
            source,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct SnapshotDto {
    version: u32,
    revision: u64,
    scene: SceneDto,
    palettes: Vec<PaletteDto>,
    authoring: AuthoringDto,
}

impl SnapshotDto {
    fn from_document(document: &TileMapDocument) -> Result<Self, TilePersistenceError> {
        let palettes = document
            .palettes
            .iter()
            .map(PaletteDto::from_palette)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            version: TILE_SNAPSHOT_VERSION,
            revision: document.revision.0,
            scene: SceneDto::from_scene(&document.scene),
            palettes,
            authoring: AuthoringDto::from_authoring(&document.authoring),
        })
    }

    fn into_document(self) -> Result<TileMapDocument, TilePersistenceError> {
        if self.version != TILE_SNAPSHOT_VERSION {
            return Err(TilePersistenceError::UnsupportedVersion {
                kind: "snapshot",
                version: self.version,
            });
        }
        let mut palettes = TilePaletteStore::new();
        for palette in self.palettes {
            palettes.insert(palette.into_palette());
        }
        let mut document =
            TileMapDocument::new(self.scene.into_scene()).with_palette_store(palettes);
        document.authoring = self.authoring.into_authoring();
        document.revision = TileDocumentRevision(self.revision);
        Ok(document)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct DeltaDto {
    version: u32,
    base_scene: u64,
    base_revision: u64,
    changes: EditSummaryDto,
}

impl DeltaDto {
    fn from_delta(delta: &TileMapDelta) -> Self {
        Self {
            version: TILE_DELTA_VERSION,
            base_scene: delta.base_scene.0,
            base_revision: delta.base_revision.0,
            changes: EditSummaryDto::from_summary(&delta.changes),
        }
    }

    fn into_delta(self) -> TileMapDelta {
        TileMapDelta {
            base_scene: TileMapId(self.base_scene),
            base_revision: TileDocumentRevision(self.base_revision),
            changes: self.changes.into_summary(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct SceneDto {
    id: u64,
    name: String,
    grid: GridDto,
    size: SizeDto,
    palettes: Vec<u32>,
    layers: Vec<LayerDto>,
    objects: Vec<ObjectDto>,
    properties: Vec<PropertyDto>,
}

impl SceneDto {
    fn from_scene(scene: &TileMap) -> Self {
        Self {
            id: scene.id.0,
            name: scene.name.clone(),
            grid: GridDto::from_grid(&scene.grid),
            size: SizeDto::from_size(scene.size),
            palettes: scene.palettes.iter().map(|id| id.0).collect(),
            layers: scene.layers.iter().map(LayerDto::from_layer).collect(),
            objects: scene.objects.iter().map(ObjectDto::from_object).collect(),
            properties: properties_to_dto(&scene.properties),
        }
    }

    fn into_scene(self) -> TileMap {
        let mut scene = TileMap::new(
            TileMapId(self.id),
            self.name,
            self.grid.into_grid(),
            self.size.into_size(),
        );
        scene.palettes = self.palettes.into_iter().map(PaletteId).collect();
        scene.layers = self.layers.into_iter().map(LayerDto::into_layer).collect();
        for object in self.objects {
            let object = object.into_object();
            let id = scene.objects.insert(object.clone());
            if let Some(layer) = scene.layer_mut(object.layer) {
                layer.add_object_id(id);
            }
        }
        scene.properties = properties_from_dto(self.properties);
        scene
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct GridDto {
    orientation: String,
    cell_size: [u32; 2],
    origin: String,
    render_order: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    stagger_axis: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    stagger_index: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    hex_side_length: Option<u32>,
}

impl GridDto {
    fn from_grid(grid: &GridSpec) -> Self {
        Self {
            orientation: grid_orientation_name(grid.orientation).to_string(),
            cell_size: grid.cell_size,
            origin: grid_origin_name(grid.origin).to_string(),
            render_order: render_order_name(grid.render_order).to_string(),
            stagger_axis: grid.stagger_axis.map(stagger_axis_name).map(str::to_string),
            stagger_index: grid
                .stagger_index
                .map(stagger_index_name)
                .map(str::to_string),
            hex_side_length: grid.hex_side_length,
        }
    }

    fn into_grid(self) -> GridSpec {
        let mut grid = GridSpec::new(parse_grid_orientation(&self.orientation), self.cell_size);
        grid.origin = parse_grid_origin(&self.origin);
        grid.render_order = parse_render_order(&self.render_order);
        grid.stagger_axis = self.stagger_axis.as_deref().map(parse_stagger_axis);
        grid.stagger_index = self.stagger_index.as_deref().map(parse_stagger_index);
        grid.hex_side_length = self.hex_side_length;
        grid
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
struct SizeDto {
    width: u32,
    height: u32,
}

impl SizeDto {
    fn from_size(size: TileMapSize) -> Self {
        Self {
            width: size.width,
            height: size.height,
        }
    }

    fn into_size(self) -> TileMapSize {
        TileMapSize::new(self.width, self.height)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct LayerDto {
    id: u32,
    name: String,
    role: LayerRoleDto,
    kind: String,
    visible: bool,
    editable: bool,
    opacity: f32,
    offset: [f32; 2],
    parallax: [f32; 2],
    data: LayerDataDto,
    properties: Vec<PropertyDto>,
}

impl LayerDto {
    fn from_layer(layer: &TileLayer) -> Self {
        Self {
            id: layer.id.0,
            name: layer.name.clone(),
            role: LayerRoleDto::from_role(&layer.role),
            kind: layer_kind_name(layer.kind).to_string(),
            visible: layer.visible,
            editable: layer.editable,
            opacity: layer.opacity,
            offset: layer.offset,
            parallax: layer.parallax,
            data: LayerDataDto::from_layer_data(&layer.data),
            properties: properties_to_dto(&layer.properties),
        }
    }

    fn into_layer(self) -> TileLayer {
        TileLayer {
            id: LayerId(self.id),
            name: self.name,
            role: self.role.into_role(),
            kind: parse_layer_kind(&self.kind),
            visible: self.visible,
            editable: self.editable,
            opacity: self.opacity,
            offset: self.offset,
            parallax: self.parallax,
            data: self.data.into_layer_data(),
            properties: properties_from_dto(self.properties),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
enum LayerRoleDto {
    Ground,
    Detail,
    Props,
    Walls,
    Upper,
    Collision,
    Gameplay,
    Preview,
    Custom(String),
}

impl LayerRoleDto {
    fn from_role(role: &LayerRole) -> Self {
        match role {
            LayerRole::Ground => Self::Ground,
            LayerRole::Detail => Self::Detail,
            LayerRole::Props => Self::Props,
            LayerRole::Walls => Self::Walls,
            LayerRole::Upper => Self::Upper,
            LayerRole::Collision => Self::Collision,
            LayerRole::Gameplay => Self::Gameplay,
            LayerRole::Preview => Self::Preview,
            LayerRole::Custom(value) => Self::Custom(value.clone()),
        }
    }

    fn into_role(self) -> LayerRole {
        match self {
            Self::Ground => LayerRole::Ground,
            Self::Detail => LayerRole::Detail,
            Self::Props => LayerRole::Props,
            Self::Walls => LayerRole::Walls,
            Self::Upper => LayerRole::Upper,
            Self::Collision => LayerRole::Collision,
            Self::Gameplay => LayerRole::Gameplay,
            Self::Preview => LayerRole::Preview,
            Self::Custom(value) => LayerRole::Custom(value),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
enum LayerDataDto {
    Tiles { tiles: Vec<CellTileDto> },
    Objects { objects: Vec<u64> },
    Collision { occupied: Vec<CellRectDto> },
    Metadata { cells: Vec<MetadataCellDto> },
}

impl LayerDataDto {
    fn from_layer_data(data: &LayerData) -> Self {
        match data {
            LayerData::Tiles(data) => Self::Tiles {
                tiles: data
                    .tiles
                    .iter()
                    .map(|(cell, tile)| CellTileDto {
                        cell: CellDto::from_cell(cell),
                        tile: SceneTileDto::from_tile(tile),
                    })
                    .collect(),
            },
            LayerData::Objects(data) => Self::Objects {
                objects: data.objects.iter().map(|id| id.0).collect(),
            },
            LayerData::Collision(data) => Self::Collision {
                occupied: data
                    .occupied
                    .iter()
                    .copied()
                    .map(CellRectDto::from_rect)
                    .collect(),
            },
            LayerData::Metadata(data) => Self::Metadata {
                cells: data
                    .cells
                    .iter()
                    .map(|(cell, properties)| MetadataCellDto {
                        cell: CellDto::from_cell(*cell),
                        properties: properties_to_dto(properties),
                    })
                    .collect(),
            },
        }
    }

    fn into_layer_data(self) -> LayerData {
        match self {
            Self::Tiles { tiles } => {
                let mut data = TileLayerData::default();
                for cell_tile in tiles {
                    let _ = data
                        .tiles
                        .set(cell_tile.cell.into_cell(), Some(cell_tile.tile.into_tile()));
                }
                LayerData::Tiles(data)
            }
            Self::Objects { objects } => LayerData::Objects(ObjectLayerData {
                objects: objects.into_iter().map(TileObjectId).collect(),
            }),
            Self::Collision { occupied } => LayerData::Collision(CollisionLayerData {
                occupied: occupied.into_iter().map(CellRectDto::into_rect).collect(),
            }),
            Self::Metadata { cells } => {
                let mut data = MetadataLayerData::default();
                for cell in cells {
                    data.cells
                        .insert(cell.cell.into_cell(), properties_from_dto(cell.properties));
                }
                LayerData::Metadata(data)
            }
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct MetadataCellDto {
    cell: CellDto,
    properties: Vec<PropertyDto>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct CellTileDto {
    cell: CellDto,
    tile: SceneTileDto,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
struct CellDto {
    x: i32,
    y: i32,
}

impl CellDto {
    fn from_cell(cell: CellCoord) -> Self {
        Self {
            x: cell.x,
            y: cell.y,
        }
    }

    fn into_cell(self) -> CellCoord {
        CellCoord::new(self.x, self.y)
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
struct CellRectDto {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

impl CellRectDto {
    fn from_rect(rect: CellRect) -> Self {
        Self {
            x: rect.min.x,
            y: rect.min.y,
            width: rect.size[0],
            height: rect.size[1],
        }
    }

    fn into_rect(self) -> CellRect {
        CellRect::new(self.x, self.y, self.width, self.height)
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
struct SceneTileDto {
    tile_ref: TileRefDto,
    flags: u8,
    tint: [f32; 4],
}

impl SceneTileDto {
    fn from_tile(tile: SceneTile) -> Self {
        Self {
            tile_ref: TileRefDto::from_ref(tile.tile_ref),
            flags: tile.flags.bits(),
            tint: tile.tint.to_array(),
        }
    }

    fn into_tile(self) -> SceneTile {
        SceneTile {
            tile_ref: self.tile_ref.into_ref(),
            flags: TileFlags::from_bits_truncate(self.flags),
            tint: Color::from(self.tint),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
struct TileRefDto {
    palette: u32,
    tile: u32,
}

impl TileRefDto {
    fn from_ref(value: TileRef) -> Self {
        Self {
            palette: value.palette.0,
            tile: value.tile.0,
        }
    }

    fn into_ref(self) -> TileRef {
        TileRef::new(PaletteId(self.palette), TileDefId(self.tile))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ObjectDto {
    id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    prototype: Option<u64>,
    layer: u32,
    cell: CellDto,
    orientation: String,
    footprint: [u32; 2],
    visual: ObjectVisualDto,
    properties: Vec<PropertyDto>,
}

impl ObjectDto {
    fn from_object(object: &TileObject) -> Self {
        Self {
            id: object.id.0,
            prototype: object.prototype.map(|id| id.0),
            layer: object.layer.0,
            cell: CellDto::from_cell(object.cell),
            orientation: tile_direction_name(object.orientation).to_string(),
            footprint: object.footprint.size,
            visual: ObjectVisualDto::from_visual(&object.visual),
            properties: properties_to_dto(&object.properties),
        }
    }

    fn into_object(self) -> TileObject {
        TileObject {
            id: TileObjectId(self.id),
            prototype: self.prototype.map(ObjectPrototypeId),
            layer: LayerId(self.layer),
            cell: self.cell.into_cell(),
            orientation: parse_tile_direction(&self.orientation),
            footprint: Footprint {
                size: self.footprint,
            },
            visual: self.visual.into_visual(),
            properties: properties_from_dto(self.properties),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
enum ObjectVisualDto {
    Tile(TileRefDto),
    MultiTile(Vec<ObjectVisualTileDto>),
    Sprite { key: String },
    None,
}

impl ObjectVisualDto {
    fn from_visual(visual: &ObjectVisual) -> Self {
        match visual {
            ObjectVisual::Tile(tile_ref) => Self::Tile(TileRefDto::from_ref(*tile_ref)),
            ObjectVisual::MultiTile(tiles) => Self::MultiTile(
                tiles
                    .iter()
                    .map(|tile| ObjectVisualTileDto {
                        offset: tile.offset,
                        tile_ref: TileRefDto::from_ref(tile.tile_ref),
                    })
                    .collect(),
            ),
            ObjectVisual::Sprite(sprite) => Self::Sprite {
                key: sprite.key.clone(),
            },
            ObjectVisual::None => Self::None,
        }
    }

    fn into_visual(self) -> ObjectVisual {
        match self {
            Self::Tile(tile_ref) => ObjectVisual::Tile(tile_ref.into_ref()),
            Self::MultiTile(tiles) => ObjectVisual::MultiTile(
                tiles
                    .into_iter()
                    .map(|tile| ObjectVisualTile {
                        offset: tile.offset,
                        tile_ref: tile.tile_ref.into_ref(),
                    })
                    .collect(),
            ),
            Self::Sprite { key } => ObjectVisual::Sprite(SpriteVisualRef { key }),
            Self::None => ObjectVisual::None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ObjectVisualTileDto {
    offset: [i32; 2],
    tile_ref: TileRefDto,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct PaletteDto {
    id: u32,
    name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source: Option<PathBuf>,
    texture: TextureSourceDto,
    tiles: Vec<TileDefDto>,
    properties: Vec<PropertyDto>,
}

impl PaletteDto {
    fn from_palette(palette: &TilePalette) -> Result<Self, TilePersistenceError> {
        Ok(Self {
            id: palette.id.0,
            name: palette.name.clone(),
            source: palette.source.as_ref().map(|source| source.path.clone()),
            texture: TextureSourceDto::from_source(palette.id, &palette.texture)?,
            tiles: palette.tiles.iter().map(TileDefDto::from_tile).collect(),
            properties: properties_to_dto(&palette.properties),
        })
    }

    fn into_palette(self) -> TilePalette {
        TilePalette {
            id: PaletteId(self.id),
            name: self.name,
            source: self.source.map(AssetSource::new),
            texture: self.texture.into_source(),
            tiles: self.tiles.into_iter().map(TileDefDto::into_tile).collect(),
            properties: properties_from_dto(self.properties),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
enum TextureSourceDto {
    None,
    Image(PathBuf),
    ImageCollectionAtlas {
        size: [u32; 2],
        tiles: Vec<AtlasImageSourceDto>,
    },
}

impl TextureSourceDto {
    fn from_source(
        palette: PaletteId,
        source: &TileTextureSource,
    ) -> Result<Self, TilePersistenceError> {
        match source {
            TileTextureSource::None => Ok(Self::None),
            TileTextureSource::Image(path) => Ok(Self::Image(path.clone())),
            TileTextureSource::ImageCollectionAtlas { size, tiles } => {
                Ok(Self::ImageCollectionAtlas {
                    size: *size,
                    tiles: tiles
                        .iter()
                        .map(|source| AtlasImageSourceDto {
                            tile: source.tile.0,
                            image: source.image.clone(),
                            source_rect: RectDto::from_rect(source.source_rect),
                        })
                        .collect(),
                })
            }
            TileTextureSource::Texture { .. } => {
                Err(TilePersistenceError::RuntimeTextureSource { palette })
            }
        }
    }

    fn into_source(self) -> TileTextureSource {
        match self {
            Self::None => TileTextureSource::None,
            Self::Image(path) => TileTextureSource::Image(path),
            Self::ImageCollectionAtlas { size, tiles } => TileTextureSource::ImageCollectionAtlas {
                size,
                tiles: tiles
                    .into_iter()
                    .map(|source| TileAtlasImageSource {
                        tile: TileDefId(source.tile),
                        image: source.image,
                        source_rect: source.source_rect.into_rect(),
                    })
                    .collect(),
            },
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct AtlasImageSourceDto {
    tile: u32,
    image: PathBuf,
    source_rect: RectDto,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct TileDefDto {
    id: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    source_rect: RectDto,
    draw_size: [u32; 2],
    draw_offset: [i32; 2],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    animation: Option<TileAnimationDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    collision: Option<TileCollisionDto>,
    properties: Vec<PropertyDto>,
}

impl TileDefDto {
    fn from_tile(tile: &TileDef) -> Self {
        Self {
            id: tile.id.0,
            name: tile.name.clone(),
            source_rect: RectDto::from_rect(tile.source_rect),
            draw_size: tile.draw_size,
            draw_offset: tile.draw_offset,
            animation: tile
                .animation
                .as_ref()
                .map(TileAnimationDto::from_animation),
            collision: tile
                .collision
                .as_ref()
                .map(TileCollisionDto::from_collision),
            properties: properties_to_dto(&tile.properties),
        }
    }

    fn into_tile(self) -> TileDef {
        TileDef {
            id: TileDefId(self.id),
            name: self.name,
            source_rect: self.source_rect.into_rect(),
            draw_size: self.draw_size,
            draw_offset: self.draw_offset,
            animation: self.animation.map(TileAnimationDto::into_animation),
            collision: self.collision.map(TileCollisionDto::into_collision),
            properties: properties_from_dto(self.properties),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
struct RectDto {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

impl RectDto {
    fn from_rect(rect: RectU) -> Self {
        Self {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
        }
    }

    fn into_rect(self) -> RectU {
        RectU::new(self.x, self.y, self.width, self.height)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct TileAnimationDto {
    frames: Vec<TileAnimationFrameDto>,
}

impl TileAnimationDto {
    fn from_animation(animation: &TileAnimation) -> Self {
        Self {
            frames: animation
                .frames
                .iter()
                .map(|frame| TileAnimationFrameDto {
                    tile: frame.tile.0,
                    duration_ms: frame.duration_ms,
                })
                .collect(),
        }
    }

    fn into_animation(self) -> TileAnimation {
        TileAnimation {
            frames: self
                .frames
                .into_iter()
                .map(|frame| TileAnimationFrame {
                    tile: TileDefId(frame.tile),
                    duration_ms: frame.duration_ms,
                })
                .collect(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct TileAnimationFrameDto {
    tile: u32,
    duration_ms: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
enum TileCollisionDto {
    Solid,
    Rect(RectDto),
    Polygon(Vec<[f32; 2]>),
}

impl TileCollisionDto {
    fn from_collision(collision: &TileCollision) -> Self {
        match collision {
            TileCollision::Solid => Self::Solid,
            TileCollision::Rect(rect) => Self::Rect(RectDto::from_rect(*rect)),
            TileCollision::Polygon(points) => Self::Polygon(points.clone()),
        }
    }

    fn into_collision(self) -> TileCollision {
        match self {
            Self::Solid => TileCollision::Solid,
            Self::Rect(rect) => TileCollision::Rect(rect.into_rect()),
            Self::Polygon(points) => TileCollision::Polygon(points),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct AuthoringDto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    format: Option<AuthoringFormatDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source: Option<PathBuf>,
    properties: Vec<PropertyDto>,
}

impl AuthoringDto {
    fn from_authoring(authoring: &TileAuthoringMetadata) -> Self {
        Self {
            format: authoring
                .format
                .as_ref()
                .map(AuthoringFormatDto::from_format),
            source: authoring.source.as_ref().map(|source| source.path.clone()),
            properties: properties_to_dto(&authoring.properties),
        }
    }

    fn into_authoring(self) -> TileAuthoringMetadata {
        TileAuthoringMetadata {
            format: self.format.map(AuthoringFormatDto::into_format),
            source: self.source.map(AssetSource::new),
            properties: properties_from_dto(self.properties),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
enum AuthoringFormatDto {
    Tiled,
    Custom(String),
}

impl AuthoringFormatDto {
    fn from_format(format: &TileAuthoringFormat) -> Self {
        match format {
            TileAuthoringFormat::Tiled => Self::Tiled,
            TileAuthoringFormat::Custom(value) => Self::Custom(value.clone()),
        }
    }

    fn into_format(self) -> TileAuthoringFormat {
        match self {
            Self::Tiled => TileAuthoringFormat::Tiled,
            Self::Custom(value) => TileAuthoringFormat::Custom(value),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct EditSummaryDto {
    dirty_cells: Vec<DirtyCellDto>,
    dirty_regions: Vec<DirtyRegionDto>,
    tile_changes: Vec<TileChangeDto>,
    changed_layers: Vec<u32>,
    object_changes: Vec<ObjectChangeDto>,
    property_changes: Vec<PropertyChangeDto>,
}

impl EditSummaryDto {
    fn from_summary(summary: &TileMapEditSummary) -> Self {
        Self {
            dirty_cells: summary
                .dirty_cells
                .iter()
                .map(|dirty| DirtyCellDto {
                    layer: dirty.layer.0,
                    cell: CellDto::from_cell(dirty.cell),
                })
                .collect(),
            dirty_regions: summary
                .dirty_regions
                .iter()
                .map(|dirty| DirtyRegionDto {
                    layer: dirty.layer.0,
                    rect: CellRectDto::from_rect(dirty.rect),
                })
                .collect(),
            tile_changes: summary
                .tile_changes
                .iter()
                .map(TileChangeDto::from_change)
                .collect(),
            changed_layers: summary.changed_layers.iter().map(|layer| layer.0).collect(),
            object_changes: summary
                .object_changes
                .iter()
                .map(ObjectChangeDto::from_change)
                .collect(),
            property_changes: summary
                .property_changes
                .iter()
                .map(PropertyChangeDto::from_change)
                .collect(),
        }
    }

    fn into_summary(self) -> TileMapEditSummary {
        TileMapEditSummary {
            dirty_cells: self
                .dirty_cells
                .into_iter()
                .map(|dirty| DirtyCell {
                    layer: LayerId(dirty.layer),
                    cell: dirty.cell.into_cell(),
                })
                .collect(),
            dirty_regions: self
                .dirty_regions
                .into_iter()
                .map(|dirty| DirtyRegion {
                    layer: LayerId(dirty.layer),
                    rect: dirty.rect.into_rect(),
                })
                .collect(),
            tile_changes: self
                .tile_changes
                .into_iter()
                .map(TileChangeDto::into_change)
                .collect(),
            changed_layers: self.changed_layers.into_iter().map(LayerId).collect(),
            object_changes: self
                .object_changes
                .into_iter()
                .map(ObjectChangeDto::into_change)
                .collect(),
            property_changes: self
                .property_changes
                .into_iter()
                .map(PropertyChangeDto::into_change)
                .collect(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct DirtyCellDto {
    layer: u32,
    cell: CellDto,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct DirtyRegionDto {
    layer: u32,
    rect: CellRectDto,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct TileChangeDto {
    layer: u32,
    cell: CellDto,
    old: Option<SceneTileDto>,
    new: Option<SceneTileDto>,
}

impl TileChangeDto {
    fn from_change(change: &TileChange) -> Self {
        Self {
            layer: change.layer.0,
            cell: CellDto::from_cell(change.cell),
            old: change.old.map(SceneTileDto::from_tile),
            new: change.new.map(SceneTileDto::from_tile),
        }
    }

    fn into_change(self) -> TileChange {
        TileChange {
            layer: LayerId(self.layer),
            cell: self.cell.into_cell(),
            old: self.old.map(SceneTileDto::into_tile),
            new: self.new.map(SceneTileDto::into_tile),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
enum ObjectChangeDto {
    Created {
        object: ObjectDto,
    },
    Removed {
        object: ObjectDto,
    },
    Moved {
        id: u64,
        from_layer: u32,
        to_layer: u32,
        from_cell: CellDto,
        to_cell: CellDto,
    },
    VisualChanged {
        id: u64,
        old: ObjectVisualDto,
        new: ObjectVisualDto,
    },
}

impl ObjectChangeDto {
    fn from_change(change: &ObjectChange) -> Self {
        match change {
            ObjectChange::Created { object } => Self::Created {
                object: ObjectDto::from_object(object),
            },
            ObjectChange::Removed { object } => Self::Removed {
                object: ObjectDto::from_object(object),
            },
            ObjectChange::Moved {
                id,
                from_layer,
                to_layer,
                from_cell,
                to_cell,
            } => Self::Moved {
                id: id.0,
                from_layer: from_layer.0,
                to_layer: to_layer.0,
                from_cell: CellDto::from_cell(*from_cell),
                to_cell: CellDto::from_cell(*to_cell),
            },
            ObjectChange::VisualChanged { id, old, new } => Self::VisualChanged {
                id: id.0,
                old: ObjectVisualDto::from_visual(old),
                new: ObjectVisualDto::from_visual(new),
            },
        }
    }

    fn into_change(self) -> ObjectChange {
        match self {
            Self::Created { object } => ObjectChange::Created {
                object: object.into_object(),
            },
            Self::Removed { object } => ObjectChange::Removed {
                object: object.into_object(),
            },
            Self::Moved {
                id,
                from_layer,
                to_layer,
                from_cell,
                to_cell,
            } => ObjectChange::Moved {
                id: TileObjectId(id),
                from_layer: LayerId(from_layer),
                to_layer: LayerId(to_layer),
                from_cell: from_cell.into_cell(),
                to_cell: to_cell.into_cell(),
            },
            Self::VisualChanged { id, old, new } => ObjectChange::VisualChanged {
                id: TileObjectId(id),
                old: old.into_visual(),
                new: new.into_visual(),
            },
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct PropertyChangeDto {
    target: PropertyTargetDto,
    key: String,
    old: Option<PropertyValueDto>,
    new: Option<PropertyValueDto>,
}

impl PropertyChangeDto {
    fn from_change(change: &PropertyChange) -> Self {
        Self {
            target: PropertyTargetDto::from_target(&change.target),
            key: change.key.clone(),
            old: change.old.as_ref().map(PropertyValueDto::from_value),
            new: change.new.as_ref().map(PropertyValueDto::from_value),
        }
    }

    fn into_change(self) -> PropertyChange {
        PropertyChange {
            target: self.target.into_target(),
            key: self.key,
            old: self.old.map(PropertyValueDto::into_value),
            new: self.new.map(PropertyValueDto::into_value),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
enum PropertyTargetDto {
    Scene,
    Layer(u32),
    Object(u64),
}

impl PropertyTargetDto {
    fn from_target(target: &PropertyTarget) -> Self {
        match target {
            PropertyTarget::Scene => Self::Scene,
            PropertyTarget::Layer(layer) => Self::Layer(layer.0),
            PropertyTarget::Object(object) => Self::Object(object.0),
        }
    }

    fn into_target(self) -> PropertyTarget {
        match self {
            Self::Scene => PropertyTarget::Scene,
            Self::Layer(layer) => PropertyTarget::Layer(LayerId(layer)),
            Self::Object(object) => PropertyTarget::Object(TileObjectId(object)),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct PropertyDto {
    key: String,
    value: PropertyValueDto,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
enum PropertyValueDto {
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Color([f32; 4]),
    File(PathBuf),
    Object(u64),
}

impl PropertyValueDto {
    fn from_value(value: &PropertyValue) -> Self {
        match value {
            PropertyValue::Bool(value) => Self::Bool(*value),
            PropertyValue::Int(value) => Self::Int(*value),
            PropertyValue::Float(value) => Self::Float(*value),
            PropertyValue::String(value) => Self::String(value.clone()),
            PropertyValue::Color(value) => Self::Color(value.to_array()),
            PropertyValue::File(value) => Self::File(value.clone()),
            PropertyValue::Object(value) => Self::Object(*value),
        }
    }

    fn into_value(self) -> PropertyValue {
        match self {
            Self::Bool(value) => PropertyValue::Bool(value),
            Self::Int(value) => PropertyValue::Int(value),
            Self::Float(value) => PropertyValue::Float(value),
            Self::String(value) => PropertyValue::String(value),
            Self::Color(value) => PropertyValue::Color(Color::from(value)),
            Self::File(value) => PropertyValue::File(value),
            Self::Object(value) => PropertyValue::Object(value),
        }
    }
}

fn properties_to_dto(properties: &PropertyBag) -> Vec<PropertyDto> {
    properties
        .iter()
        .map(|(key, value)| PropertyDto {
            key: key.to_string(),
            value: PropertyValueDto::from_value(value),
        })
        .collect()
}

fn properties_from_dto(properties: Vec<PropertyDto>) -> PropertyBag {
    let mut bag = PropertyBag::new();
    for property in properties {
        bag.insert(property.key, property.value.into_value());
    }
    bag
}

pub(crate) fn apply_delta_to_document(
    document: &mut TileMapDocument,
    delta: &TileMapDelta,
) -> Result<TileMapEditSummary, TilePersistenceError> {
    if document.scene.id != delta.base_scene {
        return Err(TilePersistenceError::SceneMismatch {
            document: document.scene.id,
            delta: delta.base_scene,
        });
    }
    if delta.base_revision > document.revision {
        return Err(TilePersistenceError::FutureDelta {
            base_revision: delta.base_revision,
            current_revision: document.revision,
        });
    }

    let mut session = TileMapEditSession::new(&mut document.scene);
    apply_summary_forward(&mut session, &delta.changes);
    let summary = session.finish();
    if !summary.is_empty() {
        document.revision = document.revision.next();
    }
    Ok(summary)
}

fn apply_summary_forward(session: &mut TileMapEditSession<'_>, summary: &TileMapEditSummary) {
    for change in &summary.tile_changes {
        session.set_tile(change.layer, change.cell, change.new);
    }
    for change in &summary.object_changes {
        match change {
            ObjectChange::Created { object } => {
                session.place_object(object.layer, object.clone());
            }
            ObjectChange::Removed { object } => {
                session.remove_object(object.id);
            }
            ObjectChange::Moved {
                id,
                to_layer,
                to_cell,
                ..
            } => session.move_object(*id, *to_layer, *to_cell),
            ObjectChange::VisualChanged { id, new, .. } => {
                session.set_object_visual(*id, new.clone());
            }
        }
    }
    for change in &summary.property_changes {
        match &change.new {
            Some(value) => session.set_property(change.target.clone(), &change.key, value.clone()),
            None => session.remove_property(change.target.clone(), &change.key),
        }
    }
}

fn grid_orientation_name(value: GridOrientation) -> &'static str {
    match value {
        GridOrientation::Orthogonal => "orthogonal",
        GridOrientation::Isometric => "isometric",
        GridOrientation::Staggered => "staggered",
        GridOrientation::Hexagonal => "hexagonal",
    }
}

fn parse_grid_orientation(value: &str) -> GridOrientation {
    match value {
        "isometric" => GridOrientation::Isometric,
        "staggered" => GridOrientation::Staggered,
        "hexagonal" => GridOrientation::Hexagonal,
        _ => GridOrientation::Orthogonal,
    }
}

fn grid_origin_name(value: GridOrigin) -> &'static str {
    match value {
        GridOrigin::TopLeft => "top_left",
        GridOrigin::BottomLeft => "bottom_left",
        GridOrigin::Center => "center",
    }
}

fn parse_grid_origin(value: &str) -> GridOrigin {
    match value {
        "bottom_left" => GridOrigin::BottomLeft,
        "center" => GridOrigin::Center,
        _ => GridOrigin::TopLeft,
    }
}

fn render_order_name(value: TilemapRenderOrder) -> &'static str {
    match value {
        TilemapRenderOrder::RightDown => "right_down",
        TilemapRenderOrder::RightUp => "right_up",
        TilemapRenderOrder::LeftDown => "left_down",
        TilemapRenderOrder::LeftUp => "left_up",
    }
}

fn parse_render_order(value: &str) -> TilemapRenderOrder {
    match value {
        "right_up" => TilemapRenderOrder::RightUp,
        "left_down" => TilemapRenderOrder::LeftDown,
        "left_up" => TilemapRenderOrder::LeftUp,
        _ => TilemapRenderOrder::RightDown,
    }
}

fn stagger_axis_name(value: StaggerAxis) -> &'static str {
    match value {
        StaggerAxis::X => "x",
        StaggerAxis::Y => "y",
    }
}

fn parse_stagger_axis(value: &str) -> StaggerAxis {
    match value {
        "x" => StaggerAxis::X,
        _ => StaggerAxis::Y,
    }
}

fn stagger_index_name(value: StaggerIndex) -> &'static str {
    match value {
        StaggerIndex::Odd => "odd",
        StaggerIndex::Even => "even",
    }
}

fn parse_stagger_index(value: &str) -> StaggerIndex {
    match value {
        "even" => StaggerIndex::Even,
        _ => StaggerIndex::Odd,
    }
}

fn layer_kind_name(value: LayerKind) -> &'static str {
    match value {
        LayerKind::Tiles => "tiles",
        LayerKind::Objects => "objects",
        LayerKind::Collision => "collision",
        LayerKind::Metadata => "metadata",
    }
}

fn parse_layer_kind(value: &str) -> LayerKind {
    match value {
        "objects" => LayerKind::Objects,
        "collision" => LayerKind::Collision,
        "metadata" => LayerKind::Metadata,
        _ => LayerKind::Tiles,
    }
}

fn tile_direction_name(value: TileDirection) -> &'static str {
    match value {
        TileDirection::North => "north",
        TileDirection::East => "east",
        TileDirection::South => "south",
        TileDirection::West => "west",
    }
}

fn parse_tile_direction(value: &str) -> TileDirection {
    match value {
        "east" => TileDirection::East,
        "south" => TileDirection::South,
        "west" => TileDirection::West,
        _ => TileDirection::North,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::render::{Color, TileFlags};

    use super::*;
    use crate::tile::{
        TileAuthoringFormat, TileMapDelta, TileMapDocument, TileMapInstance, TileMapSpawnOptions,
    };

    #[test]
    fn snapshot_round_trips_runtime_document_without_history() {
        let mut document = sample_document();
        let edit_summary = document.edit_recorded(|edit| {
            edit.set_tile(
                LayerId(10),
                CellCoord::new(1, 0),
                Some(SceneTile::new(TileRef::new(PaletteId(1), TileDefId(1)))),
            );
        });
        assert!(!edit_summary.is_empty());
        assert!(document.can_undo());
        assert_eq!(document.revision, TileDocumentRevision(1));

        let json = document
            .to_snapshot_json_string_pretty()
            .expect("snapshot json");
        let restored =
            TileMapDocument::from_snapshot_json_str(&json).expect("snapshot should restore");

        assert_eq!(restored.revision, TileDocumentRevision(1));
        assert!(!restored.can_undo());
        assert_eq!(restored.scene.id, document.scene.id);
        assert_eq!(restored.scene.name, "snapshot");
        assert_eq!(restored.scene.grid.cell_size, [16, 16]);
        assert_eq!(restored.scene.size, TileMapSize::new(4, 3));
        assert_eq!(
            restored.scene.properties.get("weather"),
            Some(&PropertyValue::String("clear".to_string()))
        );
        assert_eq!(restored.scene.layers.len(), 2);
        assert_eq!(
            restored.scene.layers[0].tile(CellCoord::new(0, 0)),
            document.scene.layers[0].tile(CellCoord::new(0, 0))
        );
        assert_eq!(
            restored.scene.layers[0].tile(CellCoord::new(1, 0)),
            document.scene.layers[0].tile(CellCoord::new(1, 0))
        );
        assert_eq!(
            restored.scene.layers[0].properties.get("biome"),
            Some(&PropertyValue::String("grass".to_string()))
        );

        let object = restored
            .scene
            .objects
            .get(TileObjectId(42))
            .expect("restored object");
        assert_eq!(object.cell, CellCoord::new(2, 1));
        assert_eq!(
            object.properties.get("name"),
            Some(&PropertyValue::String("crate".to_string()))
        );
        assert_eq!(
            restored.authoring.format,
            Some(TileAuthoringFormat::Custom("sky-test".to_string()))
        );
        assert_eq!(
            restored.authoring.properties.get("tool"),
            Some(&PropertyValue::String("unit".to_string()))
        );

        let palette = restored.palette(PaletteId(1)).expect("image palette");
        assert!(matches!(palette.texture, TileTextureSource::Image(_)));
        let collection = restored
            .palette(PaletteId(2))
            .expect("image collection palette");
        assert!(matches!(
            collection.texture,
            TileTextureSource::ImageCollectionAtlas { .. }
        ));
    }

    #[test]
    fn snapshot_file_round_trips_runtime_document() {
        let document = sample_document();
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("save.sky_tile.json");

        document
            .write_snapshot_json_file(&path)
            .expect("write snapshot");
        let restored = TileMapDocument::from_snapshot_json_file(&path).expect("read snapshot file");

        assert_eq!(restored.scene.id, document.scene.id);
        assert_eq!(restored.scene.layers.len(), document.scene.layers.len());
    }

    #[test]
    fn snapshot_rejects_runtime_texture_handles() {
        let mut document = sample_document();
        document
            .palettes
            .insert(runtime_texture_palette(PaletteId(99)));

        let error = document
            .to_snapshot_json_string()
            .expect_err("runtime texture handles must not persist");

        assert!(matches!(
            error,
            TilePersistenceError::RuntimeTextureSource {
                palette: PaletteId(99)
            }
        ));
    }

    #[test]
    fn delta_json_round_trip_applies_to_base_document() {
        let mut document = sample_document();
        let mut expected = document.clone();
        let delta = expected.edit_delta(|edit| {
            edit.set_tile(
                LayerId(10),
                CellCoord::new(1, 1),
                Some(SceneTile::new(TileRef::new(PaletteId(2), TileDefId(0)))),
            );
            edit.set_property(PropertyTarget::Scene, "weather", "rain");
        });
        assert_eq!(delta.base_scene, TileMapId(7));
        assert_eq!(delta.base_revision, TileDocumentRevision(0));
        assert_eq!(expected.revision, TileDocumentRevision(1));

        let json = delta.to_json_string_pretty().expect("delta json");
        let parsed = TileMapDelta::from_json_str(&json).expect("delta parse");
        let summary = document.apply_delta(&parsed).expect("apply delta");

        assert!(!summary.is_empty());
        assert_eq!(document.revision, TileDocumentRevision(1));
        assert_eq!(
            document.scene.layers[0].tile(CellCoord::new(1, 1)),
            expected.scene.layers[0].tile(CellCoord::new(1, 1))
        );
        assert_eq!(
            document.scene.properties.get("weather"),
            Some(&PropertyValue::String("rain".to_string()))
        );
    }

    #[test]
    fn delta_file_round_trips() {
        let mut document = sample_document();
        let delta = document.edit_delta(|edit| {
            edit.remove_property(PropertyTarget::Scene, "weather");
        });
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("player_edit.sky_tile_delta.json");

        delta.write_json_file(&path).expect("write delta");
        let restored = TileMapDelta::from_json_file(&path).expect("read delta");

        assert_eq!(restored.base_scene, TileMapId(7));
        assert_eq!(restored.changes.property_changes.len(), 1);
    }

    #[test]
    fn delta_validates_scene_and_future_revision() {
        let mut document = sample_document();
        let wrong_scene = TileMapDelta::new(
            TileMapId(999),
            TileDocumentRevision(0),
            TileMapEditSummary::default(),
        );
        assert!(matches!(
            document.apply_delta(&wrong_scene),
            Err(TilePersistenceError::SceneMismatch { .. })
        ));

        let future = TileMapDelta::new(
            document.scene.id,
            TileDocumentRevision(10),
            TileMapEditSummary::default(),
        );
        assert!(matches!(
            document.apply_delta(&future),
            Err(TilePersistenceError::FutureDelta { .. })
        ));
    }

    #[test]
    fn revision_changes_only_for_effective_document_mutations() {
        let mut document = sample_document();
        assert_eq!(document.revision, TileDocumentRevision(0));

        let no_op = document.edit_recorded(|_| {});
        assert!(no_op.is_empty());
        assert_eq!(document.revision, TileDocumentRevision(0));

        let summary = document.edit_recorded(|edit| {
            edit.set_tile(LayerId(10), CellCoord::new(0, 0), None);
        });
        assert!(!summary.is_empty());
        assert_eq!(document.revision, TileDocumentRevision(1));

        let undo = document.undo().expect("undo");
        assert!(!undo.is_empty());
        assert_eq!(document.revision, TileDocumentRevision(2));

        let redo = document.redo().expect("redo");
        assert!(!redo.is_empty());
        assert_eq!(document.revision, TileDocumentRevision(3));

        let delta = TileMapDelta::new(
            document.scene.id,
            document.revision,
            TileMapEditSummary::default(),
        );
        let applied = document.apply_delta(&delta).expect("apply no-op delta");
        assert!(applied.is_empty());
        assert_eq!(document.revision, TileDocumentRevision(3));

        let delta = document.edit_delta(|edit| {
            edit.set_property(PropertyTarget::Layer(LayerId(10)), "biome", "snow");
        });
        assert!(!delta.changes.is_empty());
        assert_eq!(document.revision, TileDocumentRevision(4));
    }

    #[test]
    fn delta_apply_allows_older_base_revision_as_best_effort() {
        let mut document = sample_document();
        let _ = document.edit_recorded(|edit| {
            edit.set_property(PropertyTarget::Scene, "weather", "wind");
        });
        assert_eq!(document.revision, TileDocumentRevision(1));

        let mut summary = TileMapEditSummary::default();
        summary.property_changes.push(PropertyChange {
            target: PropertyTarget::Scene,
            key: "difficulty".to_string(),
            old: None,
            new: Some(PropertyValue::String("hard".to_string())),
        });
        let delta = TileMapDelta::new(document.scene.id, TileDocumentRevision(0), summary);

        let applied = document.apply_delta(&delta).expect("older delta applies");

        assert!(!applied.is_empty());
        assert_eq!(document.revision, TileDocumentRevision(2));
        assert_eq!(
            document.scene.properties.get("difficulty"),
            Some(&PropertyValue::String("hard".to_string()))
        );
    }

    #[test]
    fn apply_delta_summary_can_refresh_tile_scene_instance() {
        let mut world = crate::ecs::World::new();
        world.insert_resource(crate::asset::AssetServer::with_empty_manifest(
            crate::asset::AssetConfig::default(),
        ));

        let layer = LayerId(10);
        let palette_id = PaletteId(1);
        let mut document = sample_document();
        let mut runtime_palettes = TilePaletteStore::new();
        runtime_palettes.insert(runtime_texture_palette(palette_id));
        runtime_palettes.insert(runtime_texture_palette(PaletteId(2)));
        document.palettes = runtime_palettes;

        let mut instance =
            TileMapInstance::spawn_document(&mut world, &document, TileMapSpawnOptions::default())
                .expect("document should spawn");
        let original_map = instance.scene;

        let delta = TileMapDelta::new(
            document.scene.id,
            document.revision,
            edit_summary_for_tile(
                layer,
                CellCoord::new(1, 0),
                Some(SceneTile::new(TileRef::new(palette_id, TileDefId(1)))),
            ),
        );
        let summary = document.apply_delta(&delta).expect("apply delta");
        instance
            .refresh_document_edit_summary(&mut world, &document, &summary)
            .expect("refresh after delta");

        assert_eq!(instance.scene, original_map);
        let storage = world
            .get_resource::<crate::render::TilemapStorage>()
            .expect("storage");
        let map = storage.get(instance.scene).expect("map");
        assert_eq!(
            map.tile(0, 1, 0).expect("edited cell").id,
            crate::render::TileId(1)
        );
        instance.despawn(&mut world);
    }

    fn sample_document() -> TileMapDocument {
        let ground = LayerId(10);
        let objects = LayerId(20);
        let mut scene = TileMap::new(
            TileMapId(7),
            "snapshot",
            GridSpec::orthogonal([16, 16]),
            TileMapSize::new(4, 3),
        );
        scene.palettes = vec![PaletteId(1), PaletteId(2)];
        scene.properties.insert("weather", "clear");

        let mut tile_layer = TileLayer::tiles(ground, "Ground", LayerRole::Ground);
        tile_layer.properties.insert("biome", "grass");
        tile_layer.set_tile(
            CellCoord::new(0, 0),
            Some(SceneTile {
                tile_ref: TileRef::new(PaletteId(1), TileDefId(0)),
                flags: TileFlags::FLIP_X,
                tint: Color::from([0.5, 0.75, 1.0, 1.0]),
            }),
        );
        scene.layers.push(tile_layer);

        let mut object_layer = TileLayer::objects(objects, "Objects", LayerRole::Props);
        object_layer.properties.insert("draw", "front");
        let mut object = TileObject::new(TileObjectId(42), objects, CellCoord::new(2, 1));
        object.prototype = Some(ObjectPrototypeId(5));
        object.visual = ObjectVisual::Tile(TileRef::new(PaletteId(2), TileDefId(0)));
        object.properties.insert("name", "crate");
        let object_id = scene.objects.insert(object);
        object_layer.add_object_id(object_id);
        scene.layers.push(object_layer);

        let mut palettes = TilePaletteStore::new();
        palettes.insert(image_palette());
        palettes.insert(image_collection_palette());

        let mut document = TileMapDocument::new(scene).with_palette_store(palettes);
        document.authoring.format = Some(TileAuthoringFormat::Custom("sky-test".to_string()));
        document.authoring.source = Some(AssetSource::new(PathBuf::from("source.map")));
        document.authoring.properties.insert("tool", "unit");
        document
    }

    fn image_palette() -> TilePalette {
        let mut palette = TilePalette::new(PaletteId(1), "terrain");
        palette.source = Some(AssetSource::new(PathBuf::from("terrain.tsj")));
        palette.texture = TileTextureSource::Image(PathBuf::from("terrain.png"));
        palette
            .tiles
            .push(TileDef::new(TileDefId(0), RectU::new(0, 0, 16, 16)));
        let mut second = TileDef::new(TileDefId(1), RectU::new(16, 0, 16, 16));
        second.properties.insert("slope", true);
        palette.tiles.push(second);
        palette.properties.insert("category", "ground");
        palette
    }

    fn image_collection_palette() -> TilePalette {
        let mut palette = TilePalette::new(PaletteId(2), "objects");
        palette.texture = TileTextureSource::ImageCollectionAtlas {
            size: [32, 16],
            tiles: vec![TileAtlasImageSource {
                tile: TileDefId(0),
                image: PathBuf::from("crate.png"),
                source_rect: RectU::new(0, 0, 16, 16),
            }],
        };
        palette
            .tiles
            .push(TileDef::new(TileDefId(0), RectU::new(0, 0, 16, 16)));
        palette
    }

    fn runtime_texture_palette(id: PaletteId) -> TilePalette {
        let mut palette = TilePalette::new(id, "runtime");
        palette.texture = TileTextureSource::Texture {
            handle: crate::asset::Handle::<crate::asset::TextureAsset>::new(
                crate::asset::AssetId::new(),
            ),
            size: [32, 16],
        };
        palette
            .tiles
            .push(TileDef::new(TileDefId(0), RectU::new(0, 0, 16, 16)));
        palette
            .tiles
            .push(TileDef::new(TileDefId(1), RectU::new(16, 0, 16, 16)));
        palette
    }

    fn edit_summary_for_tile(
        layer: LayerId,
        cell: CellCoord,
        new: Option<SceneTile>,
    ) -> TileMapEditSummary {
        let mut summary = TileMapEditSummary::default();
        summary.changed_layers.push(layer);
        summary.dirty_cells.push(DirtyCell { layer, cell });
        summary.tile_changes.push(TileChange {
            layer,
            cell,
            old: None,
            new,
        });
        summary
    }
}
