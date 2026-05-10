use serde::Deserialize;

#[derive(Deserialize)]
pub(super) struct TiledJsonMap {
    pub(super) orientation: String,
    #[serde(default = "default_render_order")]
    pub(super) renderorder: String,
    #[serde(default = "default_stagger_axis")]
    pub(super) staggeraxis: String,
    #[serde(default = "default_stagger_index")]
    pub(super) staggerindex: String,
    #[serde(default)]
    pub(super) hexsidelength: u32,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) tilewidth: u32,
    pub(super) tileheight: u32,
    #[serde(default)]
    pub(super) infinite: bool,
    #[serde(default)]
    pub(super) parallaxoriginx: f32,
    #[serde(default)]
    pub(super) parallaxoriginy: f32,
    #[serde(default)]
    pub(super) layers: Vec<TiledJsonLayer>,
    #[serde(default)]
    pub(super) tilesets: Vec<TiledJsonTilesetRef>,
    #[serde(default)]
    pub(super) properties: Vec<TiledJsonProperty>,
}

#[derive(Deserialize)]
pub(super) struct TiledJsonLayer {
    pub(super) name: String,
    #[serde(rename = "type")]
    pub(super) layer_type: String,
    #[serde(default)]
    pub(super) x: i32,
    #[serde(default)]
    pub(super) y: i32,
    pub(super) width: Option<u32>,
    pub(super) height: Option<u32>,
    #[serde(default = "default_visible")]
    pub(super) visible: bool,
    #[serde(default = "default_opacity")]
    pub(super) opacity: f32,
    #[serde(default)]
    pub(super) offsetx: f32,
    #[serde(default)]
    pub(super) offsety: f32,
    #[serde(default = "default_parallax")]
    pub(super) parallaxx: f32,
    #[serde(default = "default_parallax")]
    pub(super) parallaxy: f32,
    #[serde(default)]
    pub(super) encoding: Option<String>,
    #[serde(default)]
    pub(super) compression: Option<String>,
    #[serde(default)]
    pub(super) data: Option<TiledLayerData>,
    #[serde(default)]
    pub(super) chunks: Vec<TiledJsonChunk>,
    #[serde(default)]
    pub(super) layers: Vec<TiledJsonLayer>,
    #[serde(default)]
    pub(super) objects: Vec<TiledJsonObject>,
    #[serde(default)]
    pub(super) properties: Vec<TiledJsonProperty>,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum TiledLayerData {
    Array(Vec<u32>),
    Encoded(String),
}

#[derive(Deserialize)]
pub(super) struct TiledJsonChunk {
    pub(super) x: i32,
    pub(super) y: i32,
    pub(super) width: u32,
    pub(super) height: u32,
    #[serde(default)]
    pub(super) encoding: Option<String>,
    #[serde(default)]
    pub(super) compression: Option<String>,
    pub(super) data: TiledLayerData,
}

#[derive(Deserialize)]
pub(super) struct TiledJsonTilesetRef {
    pub(super) firstgid: u32,
    pub(super) source: Option<String>,
    pub(super) image: Option<String>,
    pub(super) tilewidth: Option<u32>,
    pub(super) tileheight: Option<u32>,
    pub(super) columns: Option<u32>,
    pub(super) tilecount: Option<u32>,
    pub(super) imagewidth: Option<u32>,
    pub(super) imageheight: Option<u32>,
    pub(super) transparentcolor: Option<String>,
    pub(super) tileoffset: Option<TiledJsonTileOffset>,
    #[serde(default)]
    pub(super) tiles: Vec<TiledJsonTile>,
    #[serde(default)]
    pub(super) margin: u32,
    #[serde(default)]
    pub(super) spacing: u32,
    #[serde(default)]
    pub(super) properties: Vec<TiledJsonProperty>,
}

#[derive(Deserialize)]
pub(super) struct TiledJsonTilesetFile {
    pub(super) image: Option<String>,
    pub(super) tilewidth: Option<u32>,
    pub(super) tileheight: Option<u32>,
    pub(super) columns: Option<u32>,
    pub(super) tilecount: Option<u32>,
    pub(super) imagewidth: Option<u32>,
    pub(super) imageheight: Option<u32>,
    pub(super) transparentcolor: Option<String>,
    pub(super) tileoffset: Option<TiledJsonTileOffset>,
    #[serde(default)]
    pub(super) tiles: Vec<TiledJsonTile>,
    #[serde(default)]
    pub(super) margin: u32,
    #[serde(default)]
    pub(super) spacing: u32,
    #[serde(default)]
    pub(super) properties: Vec<TiledJsonProperty>,
}

#[derive(Deserialize)]
pub(super) struct TiledJsonTile {
    pub(super) id: u32,
    #[serde(default)]
    pub(super) animation: Vec<TiledJsonAnimationFrame>,
    #[serde(default)]
    pub(super) properties: Vec<TiledJsonProperty>,
}

#[derive(Deserialize)]
pub(super) struct TiledJsonObject {
    pub(super) id: u32,
    #[serde(default)]
    pub(super) name: String,
    #[serde(default, rename = "type")]
    pub(super) object_type: String,
    #[serde(default, rename = "class")]
    pub(super) class_name: String,
    #[serde(default)]
    pub(super) x: f32,
    #[serde(default)]
    pub(super) y: f32,
    #[serde(default)]
    pub(super) width: f32,
    #[serde(default)]
    pub(super) height: f32,
    #[serde(default)]
    pub(super) gid: Option<u32>,
    #[serde(default)]
    pub(super) point: bool,
    #[serde(default)]
    pub(super) ellipse: bool,
    #[serde(default)]
    pub(super) polygon: Vec<TiledJsonPoint>,
    #[serde(default)]
    pub(super) polyline: Vec<TiledJsonPoint>,
    #[serde(default)]
    pub(super) template: Option<String>,
    #[serde(default)]
    pub(super) properties: Vec<TiledJsonProperty>,
}

#[derive(Deserialize)]
pub(super) struct TiledJsonPoint {
    pub(super) x: f32,
    pub(super) y: f32,
}

#[derive(Deserialize)]
pub(super) struct TiledJsonProperty {
    pub(super) name: String,
    #[serde(default, rename = "type")]
    pub(super) value_type: Option<String>,
    #[serde(default)]
    pub(super) value: serde_json::Value,
}

#[derive(Deserialize)]
pub(super) struct TiledJsonTileOffset {
    #[serde(default)]
    pub(super) x: i32,
    #[serde(default)]
    pub(super) y: i32,
}

#[derive(Deserialize)]
pub(super) struct TiledJsonAnimationFrame {
    pub(super) tileid: u32,
    pub(super) duration: u32,
}

fn default_visible() -> bool {
    true
}

fn default_opacity() -> f32 {
    1.0
}

fn default_parallax() -> f32 {
    1.0
}

fn default_render_order() -> String {
    "right-down".to_string()
}

fn default_stagger_axis() -> String {
    "y".to_string()
}

fn default_stagger_index() -> String {
    "odd".to_string()
}
