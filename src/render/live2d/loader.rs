//! Live2D model resource loader.
//!
//! Loads a complete `.model3.json` file, including the `.moc3` binary
//! and all texture PNGs, producing a [`Live2DModel`] and a vec of
//! [`Texture`](crate::render::Texture) ready for GPU rendering.

use std::path::{Path, PathBuf};

use crate::gpu::GpuContext;
use crate::render::core::texture::{Texture, TextureUploadDesc};
use crate::render::live2d::expression::Live2DExpressionPlayer;
use crate::render::live2d::model::Live2DModel;
use crate::render::live2d::motion::Live2DMotionPlayer;
use crate::render::live2d::physics::Live2DPhysics;
use crate::render::live2d::pose::Live2DPose;
use crate::render::live2d::runtime::{Live2DBreath, Live2DEyeBlink};

/// A fully loaded Live2D model resource (model + GPU textures).
pub struct Live2DModelResource {
    /// The Cubism model (CPU-side).
    pub model: Live2DModel,
    /// Optional looping idle motion player loaded from `motion3.json`.
    pub motion_player: Option<Live2DMotionPlayer>,
    /// Optional eye-blink runtime from the model settings.
    pub eye_blink: Option<Live2DEyeBlink>,
    /// Optional expression runtime loaded from `exp3.json`.
    pub expression_player: Option<Live2DExpressionPlayer>,
    /// Optional breath runtime using SakuraEngine's default parameter set.
    pub breath: Option<Live2DBreath>,
    /// Optional runtime physics loaded from `physics3.json`.
    pub physics: Option<Live2DPhysics>,
    /// Optional runtime pose state loaded from `pose3.json`.
    pub pose: Option<Live2DPose>,
    /// GPU textures referenced by drawables, indexed by texture index.
    pub textures: Vec<Texture>,
    /// Base directory of the model (for resolving relative paths).
    pub base_dir: PathBuf,
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
    /// 2. Loads the `.moc3` binary and creates the Cubism model
    /// 3. Loads all texture PNGs and uploads them to the GPU
    ///
    /// # Example
    /// ```no_run
    /// let resource = Live2DModelResource::load(
    ///     &ctx,
    ///     "assets/Haru/Haru.model3.json",
    /// )?;
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

        let model = Live2DModel::from_moc3_bytes(&moc_bytes).map_err(Live2DLoadError::Model)?;

        let motion_player = Live2DMotionPlayer::from_model_json(&json, &base_dir, &model)
            .map_err(Live2DLoadError::Motion)?;
        let eye_blink = Live2DEyeBlink::from_model_json(&json, &model);
        let expression_player = Live2DExpressionPlayer::from_model_json(&json, &base_dir, &model)
            .map_err(Live2DLoadError::Expression)?;
        let breath = Live2DBreath::from_model(&model);
        let physics = json
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

        let pose = json
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
            eprintln!(
                "[Live2D] Warning: model expects {} textures but only {} were loaded",
                expected_tex_count,
                textures.len()
            );
        }

        Ok(Self {
            model,
            motion_player,
            eye_blink,
            expression_player,
            breath,
            physics,
            pose,
            textures,
            base_dir,
        })
    }

    /// Advance runtime state for one frame.
    ///
    /// This applies the currently-supported runtime effects in roughly the same
    /// order as SakuraEngine's `csmUserModel::update()`:
    /// motion -> eye blink -> expression -> breath -> physics -> pose -> core model update.
    pub fn update(&mut self, dt: f32) {
        if let Some(motion_player) = self.motion_player.as_mut() {
            motion_player.update(&mut self.model, dt);
        }
        if let Some(eye_blink) = self.eye_blink.as_mut() {
            eye_blink.update_parameters(&mut self.model, dt);
        }
        if let Some(expression_player) = self.expression_player.as_mut() {
            expression_player.update(&mut self.model, dt);
        }
        if let Some(breath) = self.breath.as_mut() {
            breath.update_parameters(&mut self.model, dt);
        }
        if let Some(physics) = self.physics.as_mut() {
            physics.evaluate(&mut self.model, dt);
        }
        if let Some(pose) = self.pose.as_mut() {
            pose.update_parameters(&mut self.model, dt);
        }
        self.model.update();
    }

    /// Activate one named expression if the model has expressions loaded.
    pub fn set_expression(&mut self, name: &str) -> bool {
        self.expression_player
            .as_mut()
            .is_some_and(|player| player.set_expression(name))
    }

    fn load_texture(ctx: &GpuContext, path: &Path) -> Result<Texture, Live2DLoadError> {
        // Use the image crate to decode PNG → RGBA8
        let img = image::open(path)
            .map_err(|e| {
                Live2DLoadError::Texture(format!("failed to load {}: {e}", path.display()))
            })?
            .to_rgba8();

        let (w, h) = img.dimensions();
        let label = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "live2d_texture".into());

        // Live2D textures are sampled in linear space.
        Ok(Texture::from_upload_desc(
            ctx,
            TextureUploadDesc::new(w, h, &img)
                .format(wgpu::TextureFormat::Rgba8Unorm)
                .label(label),
        ))
    }
}
