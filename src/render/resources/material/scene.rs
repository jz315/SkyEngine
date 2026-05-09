/// Scene-level capabilities a material can request from the renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SceneResourceKind {
    Camera,
    Model,
    Lighting,
    Shadows,
    Gi,
    SceneDepth,
    SceneNormal,
    SceneVelocity,
    MaterialPrepass,
}

/// Declarative scene resource requirements for a material model.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct SceneResourceRequirements {
    required: Vec<SceneResourceKind>,
    optional: Vec<SceneResourceKind>,
}

impl SceneResourceRequirements {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn require(mut self, kind: SceneResourceKind) -> Self {
        push_unique(&mut self.required, kind);
        self.optional.retain(|candidate| *candidate != kind);
        self
    }

    pub fn optional(mut self, kind: SceneResourceKind) -> Self {
        if !self.required.contains(&kind) {
            push_unique(&mut self.optional, kind);
        }
        self
    }

    #[inline]
    pub fn camera(self) -> Self {
        self.require(SceneResourceKind::Camera)
    }

    #[inline]
    pub fn model(self) -> Self {
        self.require(SceneResourceKind::Model)
    }

    #[inline]
    pub fn lighting(self) -> Self {
        self.require(SceneResourceKind::Lighting)
    }

    #[inline]
    pub fn shadows(self) -> Self {
        self.require(SceneResourceKind::Shadows)
    }

    #[inline]
    pub fn shadows_optional(self) -> Self {
        self.optional(SceneResourceKind::Shadows)
    }

    #[inline]
    pub fn gi(self) -> Self {
        self.require(SceneResourceKind::Gi)
    }

    #[inline]
    pub fn gi_optional(self) -> Self {
        self.optional(SceneResourceKind::Gi)
    }

    #[inline]
    pub fn material_prepass(self) -> Self {
        self.require(SceneResourceKind::MaterialPrepass)
    }

    #[inline]
    pub fn required(&self) -> &[SceneResourceKind] {
        &self.required
    }

    #[inline]
    pub fn optional_resources(&self) -> &[SceneResourceKind] {
        &self.optional
    }

    #[inline]
    pub fn contains(&self, kind: SceneResourceKind) -> bool {
        self.required.contains(&kind) || self.optional.contains(&kind)
    }
}

fn push_unique(values: &mut Vec<SceneResourceKind>, kind: SceneResourceKind) {
    if !values.contains(&kind) {
        values.push(kind);
    }
}
