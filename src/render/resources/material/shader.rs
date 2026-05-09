use std::borrow::Cow;
use std::hash::{Hash, Hasher};

/// Shader source used by material models.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ShaderSource {
    Wgsl(Cow<'static, str>),
}

impl ShaderSource {
    #[inline]
    pub const fn wgsl(source: &'static str) -> Self {
        Self::Wgsl(Cow::Borrowed(source))
    }

    #[inline]
    pub(crate) fn wgsl_source(&self) -> &str {
        match self {
            Self::Wgsl(source) => source.as_ref(),
        }
    }
}

/// Entry points and source for a material shader program.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MaterialShaderSet {
    pub source: ShaderSource,
    pub vertex_entry: &'static str,
    pub fragment_entry: &'static str,
}

impl MaterialShaderSet {
    #[inline]
    pub const fn wgsl(source: &'static str) -> Self {
        Self {
            source: ShaderSource::wgsl(source),
            vertex_entry: "vs_main",
            fragment_entry: "fs_main",
        }
    }

    #[inline]
    pub const fn with_entries(
        mut self,
        vertex_entry: &'static str,
        fragment_entry: &'static str,
    ) -> Self {
        self.vertex_entry = vertex_entry;
        self.fragment_entry = fragment_entry;
        self
    }
}

/// Explicit shader variant identity selected from material data and frame
/// policy. Value-like material fields should not be encoded here.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct ShaderVariantKey {
    dimensions: Vec<(&'static str, u64)>,
}

impl ShaderVariantKey {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with(mut self, dimension: &'static str, value: impl Into<u64>) -> Self {
        self.dimensions.push((dimension, value.into()));
        self
    }

    #[inline]
    pub fn dimensions(&self) -> &[(&'static str, u64)] {
        &self.dimensions
    }

    pub(crate) fn stable_hash(&self) -> u64 {
        let mut hasher = rustc_hash::FxHasher::default();
        self.hash(&mut hasher);
        hasher.finish()
    }
}

/// Variant dimensions declared by a material interface for debug and
/// validation.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct ShaderVariantPolicy {
    dimensions: Vec<&'static str>,
}

impl ShaderVariantPolicy {
    #[inline]
    pub fn none() -> Self {
        Self::default()
    }

    pub fn new(dimensions: impl Into<Vec<&'static str>>) -> Self {
        Self {
            dimensions: dimensions.into(),
        }
    }

    pub fn dimension(mut self, name: &'static str) -> Self {
        self.dimensions.push(name);
        self
    }

    #[inline]
    pub fn dimensions(&self) -> &[&'static str] {
        &self.dimensions
    }
}
