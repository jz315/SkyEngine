/// Main pass routing for a material model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum MainPassMode {
    #[default]
    Opaque,
    AlphaMask,
    Transparent,
    Additive,
}

impl MainPassMode {
    #[inline]
    pub const fn is_transparent(self) -> bool {
        matches!(self, Self::Transparent | Self::Additive)
    }
}

/// Optional scene material prepass participation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MaterialPrepassMode {
    SceneMaterial,
}

/// Shadow pass participation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ShadowPassMode {
    #[default]
    None,
    Opaque,
    AlphaTest,
    Transparent,
}

/// Pass contract declared by a material model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct MaterialPassSet {
    pub main: MainPassMode,
    pub prepass: Option<MaterialPrepassMode>,
    pub shadow: ShadowPassMode,
}

impl MaterialPassSet {
    #[inline]
    pub const fn opaque() -> Self {
        Self {
            main: MainPassMode::Opaque,
            prepass: None,
            shadow: ShadowPassMode::None,
        }
    }

    #[inline]
    pub const fn transparent() -> Self {
        Self {
            main: MainPassMode::Transparent,
            prepass: None,
            shadow: ShadowPassMode::None,
        }
    }

    #[inline]
    pub const fn is_transparent(self) -> bool {
        self.main.is_transparent()
    }
}
