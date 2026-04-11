// Live2D model resource loader.
//
// Loads a complete `.model3.json` file, including the `.moc3` binary
// and all texture PNGs, producing a [`Live2DModel`] and a vec of
// [`Texture`](crate::render::Texture) ready for GPU rendering.

use std::path::{Path, PathBuf};

use crate::gpu::GpuContext;
use crate::render::core::texture::Texture;
use crate::render::live2d::model::{Live2DLayout, Live2DModel};
use crate::render::live2d::runtime::{
    Live2DBreath, Live2DExpressionPlayer, Live2DEyeBlink, Live2DLipSync, Live2DLook,
    Live2DMotionPlayer, Live2DPhysics, Live2DPose,
};

/// Named hit area resolved to a drawable index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Live2DHitArea {
    pub name: String,
    pub drawable_id: String,
    pub drawable_index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Live2DUserDataEntry {
    pub target: String,
    pub id: String,
    pub value: String,
    pub drawable_index: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Live2DDisplayNamedEntry {
    pub id: String,
    pub group_id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Live2DDisplayInfo {
    pub parameters: Vec<Live2DDisplayNamedEntry>,
    pub parameter_groups: Vec<Live2DDisplayNamedEntry>,
    pub parts: Vec<Live2DDisplayNamedEntry>,
}

/// Immutable loaded Live2D asset bundle.
///
/// This object owns static data loaded from disk and GPU textures. Runtime
/// state lives in [`crate::render::live2d::Live2DUserModel`], created via
/// [`Live2DModelResource::instantiate`].
pub struct Live2DModelResource {
    pub(crate) moc_bytes: Vec<u8>,
    pub(crate) layout: Option<Live2DLayout>,
    pub(crate) motion_player_template: Option<Live2DMotionPlayer>,
    pub(crate) eye_blink_template: Option<Live2DEyeBlink>,
    pub(crate) expression_player_template: Option<Live2DExpressionPlayer>,
    pub(crate) look_template: Option<Live2DLook>,
    pub(crate) breath_template: Option<Live2DBreath>,
    pub(crate) physics_template: Option<Live2DPhysics>,
    pub(crate) lip_sync_template: Option<Live2DLipSync>,
    pub(crate) pose_template: Option<Live2DPose>,
    pub(crate) hit_areas: Vec<Live2DHitArea>,
    pub(crate) user_data: Vec<Live2DUserDataEntry>,
    pub(crate) display_info: Option<Live2DDisplayInfo>,
    pub(crate) extra_parameter_ids: Vec<String>,
    pub(crate) textures: Vec<Texture>,
    pub(crate) base_dir: PathBuf,
}

/// Errors during model resource loading.
#[derive(Debug)]
pub enum Live2DLoadError {
    Io(std::io::Error),
    Json(String),
    Model(String),
    Motion(String),
    Expression(String),
    Physics(String),
    Pose(String),
    Texture(String),
}

impl std::fmt::Display for Live2DLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::Json(msg) => write!(f, "JSON parse error: {msg}"),
            Self::Model(msg) => write!(f, "Model error: {msg}"),
            Self::Motion(msg) => write!(f, "Motion error: {msg}"),
            Self::Expression(msg) => write!(f, "Expression error: {msg}"),
            Self::Physics(msg) => write!(f, "Physics error: {msg}"),
            Self::Pose(msg) => write!(f, "Pose error: {msg}"),
            Self::Texture(msg) => write!(f, "Texture error: {msg}"),
        }
    }
}

impl std::error::Error for Live2DLoadError {}

impl From<std::io::Error> for Live2DLoadError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl Live2DModelResource {
    /// Load a Live2D model from a `.model3.json` file.
    ///
    /// This is a synchronous operation that:
    /// 1. Parses the JSON settings file
    /// 2. Builds immutable runtime templates from the loaded model definition
    /// 3. Loads all texture PNGs and uploads them to the GPU
    ///
    /// # Example
    /// ```no_run
    /// # use sky_engine::gpu::GpuContext;
    /// # use sky_engine::render::expert::live2d::Live2DModelResource;
    /// # fn demo(ctx: &GpuContext) -> Result<(), Box<dyn std::error::Error>> {
    /// let resource = Live2DModelResource::load(
    ///     &ctx,
    ///     "assets/Haru/Haru.model3.json",
    /// )?;
    /// # let _ = resource;
    /// # Ok(())
    /// # }
    /// ```
    pub fn load(
        ctx: &GpuContext,
        model_json_path: impl AsRef<Path>,
    ) -> Result<Self, Live2DLoadError> {
        let model_json_path = model_json_path.as_ref();
        let base_dir = model_json_path
            .parent()
            .ok_or_else(|| Live2DLoadError::Json("no parent directory".into()))?
            .to_path_buf();

        // Parse .model3.json
        let json_text = std::fs::read_to_string(model_json_path)?;
        let json: serde_json::Value =
            serde_json::from_str(&json_text).map_err(|e| Live2DLoadError::Json(e.to_string()))?;

        // Extract moc file path
        let moc_filename = json
            .pointer("/FileReferences/Moc")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Live2DLoadError::Json("missing FileReferences.Moc".into()))?;

        let moc_path = base_dir.join(moc_filename);
        let moc_bytes = std::fs::read(&moc_path).map_err(|e| {
            Live2DLoadError::Io(std::io::Error::new(
                e.kind(),
                format!("failed to read moc3: {}: {e}", moc_path.display()),
            ))
        })?;

        let mut model = Live2DModel::from_moc3_bytes(&moc_bytes).map_err(Live2DLoadError::Model)?;
        let layout = Self::parse_layout(&json);
        if let Some(layout) = layout {
            model.apply_layout(&layout);
        }

        let motion_player_template =
            Live2DMotionPlayer::from_model_json(&json, &base_dir, &mut model)
                .map_err(Live2DLoadError::Motion)?;
        let eye_blink_template = Live2DEyeBlink::from_model_json(&json, &mut model);
        let expression_player_template =
            Live2DExpressionPlayer::from_model_json(&json, &base_dir, &mut model)
                .map_err(Live2DLoadError::Expression)?;
        let look_template = Live2DLook::from_model(&mut model);
        let breath_template = Live2DBreath::from_model(&mut model);
        let physics_template = json
            .pointer("/FileReferences/Physics")
            .and_then(|v| v.as_str())
            .map(|physics_filename| {
                let physics_path = base_dir.join(physics_filename);
                let physics_text = std::fs::read_to_string(&physics_path).map_err(|e| {
                    Live2DLoadError::Io(std::io::Error::new(
                        e.kind(),
                        format!("failed to read physics3: {}: {e}", physics_path.display()),
                    ))
                })?;
                Live2DPhysics::from_json_str(&physics_text, &model)
                    .map_err(Live2DLoadError::Physics)
            })
            .transpose()?;

        let pose_template = json
            .pointer("/FileReferences/Pose")
            .and_then(|v| v.as_str())
            .map(|pose_filename| {
                let pose_path = base_dir.join(pose_filename);
                let pose_text = std::fs::read_to_string(&pose_path).map_err(|e| {
                    Live2DLoadError::Io(std::io::Error::new(
                        e.kind(),
                        format!("failed to read pose3: {}: {e}", pose_path.display()),
                    ))
                })?;
                Live2DPose::from_json_str(&pose_text).map_err(Live2DLoadError::Pose)
            })
            .transpose()?;
        let lip_sync_template = Live2DLipSync::from_model_json(&json, &mut model);
        let hit_areas = Self::parse_hit_areas(&json, &model);
        let user_data = json
            .pointer("/FileReferences/UserData")
            .and_then(|v| v.as_str())
            .map(|filename| {
                let path = base_dir.join(filename);
                let text = std::fs::read_to_string(&path).map_err(|e| {
                    Live2DLoadError::Io(std::io::Error::new(
                        e.kind(),
                        format!("failed to read userdata3: {}: {e}", path.display()),
                    ))
                })?;
                Self::parse_user_data(&text, &model).map_err(Live2DLoadError::Json)
            })
            .transpose()?
            .unwrap_or_default();
        let display_info = json
            .pointer("/FileReferences/DisplayInfo")
            .and_then(|v| v.as_str())
            .map(|filename| {
                let path = base_dir.join(filename);
                let text = std::fs::read_to_string(&path).map_err(|e| {
                    Live2DLoadError::Io(std::io::Error::new(
                        e.kind(),
                        format!("failed to read cdi3: {}: {e}", path.display()),
                    ))
                })?;
                Self::parse_display_info(&text).map_err(Live2DLoadError::Json)
            })
            .transpose()?;

        // Extract texture paths
        let texture_filenames: Vec<String> = json
            .pointer("/FileReferences/Textures")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        // Load textures
        let mut textures = Vec::with_capacity(texture_filenames.len());
        for tex_filename in &texture_filenames {
            let tex_path = base_dir.join(tex_filename);
            let tex = Self::load_texture(ctx, &tex_path)?;
            textures.push(tex);
        }

        // Sanity check: model expects N textures, we loaded M
        let expected_tex_count = model.texture_count();
        if textures.len() < expected_tex_count {
            return Err(Live2DLoadError::Texture(format!(
                "model expects {expected_tex_count} textures but only {} were declared/loaded",
                textures.len()
            )));
        }

        Ok(Self {
            moc_bytes,
            layout,
            motion_player_template,
            eye_blink_template,
            expression_player_template,
            look_template,
            breath_template,
            physics_template,
            lip_sync_template,
            pose_template,
            hit_areas,
            user_data,
            display_info,
            extra_parameter_ids: model.extra_virtual_parameter_ids().to_vec(),
            textures,
            base_dir,
        })
    }
}
