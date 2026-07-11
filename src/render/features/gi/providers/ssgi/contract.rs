use super::constants::{SSGI_COMPOSITE_PASS, SSGI_FINAL_PASS, SSGI_MIP_COUNT};
use crate::render::graph::PassFlags;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SsgiSampleParams {
    pub(crate) range: f32,
    pub(crate) spread: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum SsgiResourceRole {
    SceneColorInput,
    SceneDepth,
    SceneNormal,
    SceneVelocity,
    AtlasDepth { mip: u32 },
    AtlasColor { mip: u32 },
    DepthMip { mip: u32 },
    NormalMip { mip: u32 },
    DiffuseMip { mip: u32 },
    FilteredDiffuseMip { mip: u32 },
    FinalIndirectDiffuse,
    OutputSceneColor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum SsgiBindingRole {
    SceneColor,
    SceneDepth,
    SceneNormal,
    SceneVelocity,
    Uniform,
    DeinterleaveAtlasDepthOutput,
    DeinterleaveAtlasColorOutput,
    DeinterleaveDepthOutput,
    DeinterleaveNormalOutput,
    DiffuseAtlasDepthInput,
    DiffuseAtlasColorInput,
    DiffuseNormalInput,
    DiffuseOutput,
    UpsampleLowDepthInput,
    UpsampleLowNormalInput,
    UpsampleLowDiffuseInput,
    UpsampleHighDepthInput,
    UpsampleHighNormalInput,
    UpsampleHighDiffuseInput,
    UpsampleOutput,
    FinalLowDepth,
    FinalLowNormal,
    FinalLowDiffuse,
    FinalSceneDepth,
    FinalSceneNormal,
    FinalSceneColor,
    CompositeIndirectDiffuse,
    CompositeSceneColor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SsgiShaderStage {
    DeinterleaveCompute,
    DiffuseCompute,
    UpsampleCompute,
    FinalFullscreen,
    CompositeFullscreen,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SsgiDispatchRule {
    WorkgroupsForWrite(SsgiResourceRole),
    Fullscreen,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SsgiPassKind {
    Deinterleave {
        mip_index: usize,
    },
    Diffuse {
        mip_index: usize,
    },
    Upsample {
        source_mip_index: usize,
        target_mip_index: usize,
        pass_index: usize,
    },
    FinalComposite,
    SceneComposite,
}

pub(crate) struct SsgiPassDescriptor {
    pub(crate) name: &'static str,
    pub(crate) kind: SsgiPassKind,
    pub(crate) reads: &'static [SsgiResourceRole],
    pub(crate) writes: &'static [SsgiResourceRole],
    pub(crate) bindings: &'static [SsgiBindingRole],
    pub(crate) shader: SsgiShaderStage,
    pub(crate) dispatch: SsgiDispatchRule,
    pub(crate) flags: PassFlags,
}

const SSGI_DIFFUSE_PARAMS: [SsgiSampleParams; SSGI_MIP_COUNT] = [
    SsgiSampleParams {
        range: 2.0,
        spread: 2.0,
    },
    SsgiSampleParams {
        range: 2.0,
        spread: 2.0,
    },
    SsgiSampleParams {
        range: 4.0,
        spread: 4.0,
    },
    SsgiSampleParams {
        range: 8.0,
        spread: 2.0,
    },
];

const SSGI_UPSAMPLE_PARAMS: [SsgiSampleParams; SSGI_MIP_COUNT] = [
    SsgiSampleParams {
        range: 3.0,
        spread: 2.0,
    },
    SsgiSampleParams {
        range: 2.0,
        spread: 3.0,
    },
    SsgiSampleParams {
        range: 1.0,
        spread: 2.0,
    },
    SsgiSampleParams {
        range: 1.0,
        spread: 1.0,
    },
];

impl SsgiPassKind {
    #[inline]
    pub(crate) const fn is_compute(self) -> bool {
        !matches!(self, Self::FinalComposite | Self::SceneComposite)
    }

    #[inline]
    pub(crate) const fn is_final(self) -> bool {
        matches!(self, Self::FinalComposite | Self::SceneComposite)
    }

    pub(crate) fn settings(self) -> SsgiSampleParams {
        match self {
            Self::Deinterleave { .. } => SsgiSampleParams {
                range: 1.0,
                spread: 1.0,
            },
            Self::Diffuse { mip_index } => SSGI_DIFFUSE_PARAMS[mip_index],
            Self::Upsample { pass_index, .. } => SSGI_UPSAMPLE_PARAMS[pass_index],
            Self::FinalComposite | Self::SceneComposite => SSGI_UPSAMPLE_PARAMS[SSGI_MIP_COUNT - 1],
        }
    }

    pub(crate) fn output_scale(self) -> u32 {
        match self {
            Self::Deinterleave { mip_index } | Self::Diffuse { mip_index } => {
                1u32 << (mip_index + 1)
            }
            Self::Upsample {
                target_mip_index, ..
            } => 1u32 << (target_mip_index + 1),
            Self::FinalComposite | Self::SceneComposite => 1,
        }
    }

    pub(crate) fn source_scale(self) -> u32 {
        match self {
            Self::Deinterleave { mip_index } | Self::Diffuse { mip_index } => {
                1u32 << (mip_index + 1)
            }
            Self::Upsample {
                source_mip_index, ..
            } => 1u32 << (source_mip_index + 1),
            Self::FinalComposite | Self::SceneComposite => 2,
        }
    }
}

const DEINTERLEAVE_READS: [SsgiResourceRole; 4] = [
    SsgiResourceRole::SceneColorInput,
    SsgiResourceRole::SceneDepth,
    SsgiResourceRole::SceneNormal,
    SsgiResourceRole::SceneVelocity,
];
const DEINTERLEAVE_BINDINGS: [SsgiBindingRole; 9] = [
    SsgiBindingRole::SceneColor,
    SsgiBindingRole::SceneDepth,
    SsgiBindingRole::SceneNormal,
    SsgiBindingRole::SceneVelocity,
    SsgiBindingRole::Uniform,
    SsgiBindingRole::DeinterleaveAtlasDepthOutput,
    SsgiBindingRole::DeinterleaveAtlasColorOutput,
    SsgiBindingRole::DeinterleaveDepthOutput,
    SsgiBindingRole::DeinterleaveNormalOutput,
];

const DIFFUSE_BINDINGS: [SsgiBindingRole; 5] = [
    SsgiBindingRole::DiffuseAtlasDepthInput,
    SsgiBindingRole::DiffuseAtlasColorInput,
    SsgiBindingRole::DiffuseNormalInput,
    SsgiBindingRole::Uniform,
    SsgiBindingRole::DiffuseOutput,
];

const UPSAMPLE_BINDINGS: [SsgiBindingRole; 8] = [
    SsgiBindingRole::UpsampleLowDepthInput,
    SsgiBindingRole::UpsampleLowNormalInput,
    SsgiBindingRole::UpsampleLowDiffuseInput,
    SsgiBindingRole::UpsampleHighDepthInput,
    SsgiBindingRole::UpsampleHighNormalInput,
    SsgiBindingRole::UpsampleHighDiffuseInput,
    SsgiBindingRole::Uniform,
    SsgiBindingRole::UpsampleOutput,
];

const FINAL_READS: [SsgiResourceRole; 6] = [
    SsgiResourceRole::DepthMip { mip: 0 },
    SsgiResourceRole::NormalMip { mip: 0 },
    SsgiResourceRole::FilteredDiffuseMip { mip: 0 },
    SsgiResourceRole::SceneDepth,
    SsgiResourceRole::SceneNormal,
    SsgiResourceRole::SceneColorInput,
];
const FINAL_WRITES: [SsgiResourceRole; 1] = [SsgiResourceRole::FinalIndirectDiffuse];
const FINAL_BINDINGS: [SsgiBindingRole; 7] = [
    SsgiBindingRole::FinalLowDepth,
    SsgiBindingRole::FinalLowNormal,
    SsgiBindingRole::FinalLowDiffuse,
    SsgiBindingRole::FinalSceneDepth,
    SsgiBindingRole::FinalSceneNormal,
    SsgiBindingRole::FinalSceneColor,
    SsgiBindingRole::Uniform,
];
const COMPOSITE_READS: [SsgiResourceRole; 2] = [
    SsgiResourceRole::FinalIndirectDiffuse,
    SsgiResourceRole::SceneColorInput,
];
const COMPOSITE_WRITES: [SsgiResourceRole; 1] = [SsgiResourceRole::OutputSceneColor];
const COMPOSITE_BINDINGS: [SsgiBindingRole; 3] = [
    SsgiBindingRole::CompositeIndirectDiffuse,
    SsgiBindingRole::CompositeSceneColor,
    SsgiBindingRole::Uniform,
];

macro_rules! deinterleave_descriptor {
    ($name:literal, $mip:expr, $writes:ident) => {
        SsgiPassDescriptor {
            name: $name,
            kind: SsgiPassKind::Deinterleave { mip_index: $mip },
            reads: &DEINTERLEAVE_READS,
            writes: &$writes,
            bindings: &DEINTERLEAVE_BINDINGS,
            shader: SsgiShaderStage::DeinterleaveCompute,
            dispatch: SsgiDispatchRule::WorkgroupsForWrite(SsgiResourceRole::DepthMip {
                mip: $mip as u32,
            }),
            flags: PassFlags::PREFER_ASYNC_COMPUTE.union(PassFlags::BANDWIDTH_INTENSIVE),
        }
    };
}

macro_rules! diffuse_descriptor {
    ($name:literal, $mip:expr, $reads:ident, $writes:ident) => {
        SsgiPassDescriptor {
            name: $name,
            kind: SsgiPassKind::Diffuse { mip_index: $mip },
            reads: &$reads,
            writes: &$writes,
            bindings: &DIFFUSE_BINDINGS,
            shader: SsgiShaderStage::DiffuseCompute,
            dispatch: SsgiDispatchRule::WorkgroupsForWrite(SsgiResourceRole::DiffuseMip {
                mip: $mip as u32,
            }),
            flags: PassFlags::PREFER_ASYNC_COMPUTE.union(PassFlags::COMPUTE_INTENSIVE),
        }
    };
}

macro_rules! upsample_descriptor {
    ($name:literal, $source:expr, $target:expr, $pass_index:expr, $reads:ident, $writes:ident) => {
        SsgiPassDescriptor {
            name: $name,
            kind: SsgiPassKind::Upsample {
                source_mip_index: $source,
                target_mip_index: $target,
                pass_index: $pass_index,
            },
            reads: &$reads,
            writes: &$writes,
            bindings: &UPSAMPLE_BINDINGS,
            shader: SsgiShaderStage::UpsampleCompute,
            dispatch: SsgiDispatchRule::WorkgroupsForWrite(SsgiResourceRole::FilteredDiffuseMip {
                mip: $target as u32,
            }),
            flags: PassFlags::PREFER_ASYNC_COMPUTE.union(PassFlags::BANDWIDTH_INTENSIVE),
        }
    };
}

const DEINTERLEAVE_0_WRITES: [SsgiResourceRole; 4] = [
    SsgiResourceRole::AtlasDepth { mip: 0 },
    SsgiResourceRole::AtlasColor { mip: 0 },
    SsgiResourceRole::DepthMip { mip: 0 },
    SsgiResourceRole::NormalMip { mip: 0 },
];
const DEINTERLEAVE_1_WRITES: [SsgiResourceRole; 4] = [
    SsgiResourceRole::AtlasDepth { mip: 1 },
    SsgiResourceRole::AtlasColor { mip: 1 },
    SsgiResourceRole::DepthMip { mip: 1 },
    SsgiResourceRole::NormalMip { mip: 1 },
];
const DEINTERLEAVE_2_WRITES: [SsgiResourceRole; 4] = [
    SsgiResourceRole::AtlasDepth { mip: 2 },
    SsgiResourceRole::AtlasColor { mip: 2 },
    SsgiResourceRole::DepthMip { mip: 2 },
    SsgiResourceRole::NormalMip { mip: 2 },
];
const DEINTERLEAVE_3_WRITES: [SsgiResourceRole; 4] = [
    SsgiResourceRole::AtlasDepth { mip: 3 },
    SsgiResourceRole::AtlasColor { mip: 3 },
    SsgiResourceRole::DepthMip { mip: 3 },
    SsgiResourceRole::NormalMip { mip: 3 },
];

const DIFFUSE_0_READS: [SsgiResourceRole; 3] = [
    SsgiResourceRole::AtlasDepth { mip: 0 },
    SsgiResourceRole::AtlasColor { mip: 0 },
    SsgiResourceRole::NormalMip { mip: 0 },
];
const DIFFUSE_1_READS: [SsgiResourceRole; 3] = [
    SsgiResourceRole::AtlasDepth { mip: 1 },
    SsgiResourceRole::AtlasColor { mip: 1 },
    SsgiResourceRole::NormalMip { mip: 1 },
];
const DIFFUSE_2_READS: [SsgiResourceRole; 3] = [
    SsgiResourceRole::AtlasDepth { mip: 2 },
    SsgiResourceRole::AtlasColor { mip: 2 },
    SsgiResourceRole::NormalMip { mip: 2 },
];
const DIFFUSE_3_READS: [SsgiResourceRole; 3] = [
    SsgiResourceRole::AtlasDepth { mip: 3 },
    SsgiResourceRole::AtlasColor { mip: 3 },
    SsgiResourceRole::NormalMip { mip: 3 },
];
const DIFFUSE_0_WRITES: [SsgiResourceRole; 1] = [SsgiResourceRole::DiffuseMip { mip: 0 }];
const DIFFUSE_1_WRITES: [SsgiResourceRole; 1] = [SsgiResourceRole::DiffuseMip { mip: 1 }];
const DIFFUSE_2_WRITES: [SsgiResourceRole; 1] = [SsgiResourceRole::DiffuseMip { mip: 2 }];
const DIFFUSE_3_WRITES: [SsgiResourceRole; 1] = [SsgiResourceRole::DiffuseMip { mip: 3 }];

const UPSAMPLE_3_TO_2_READS: [SsgiResourceRole; 6] = [
    SsgiResourceRole::DepthMip { mip: 3 },
    SsgiResourceRole::NormalMip { mip: 3 },
    SsgiResourceRole::DiffuseMip { mip: 3 },
    SsgiResourceRole::DepthMip { mip: 2 },
    SsgiResourceRole::NormalMip { mip: 2 },
    SsgiResourceRole::DiffuseMip { mip: 2 },
];
const UPSAMPLE_2_TO_1_READS: [SsgiResourceRole; 6] = [
    SsgiResourceRole::DepthMip { mip: 2 },
    SsgiResourceRole::NormalMip { mip: 2 },
    SsgiResourceRole::FilteredDiffuseMip { mip: 2 },
    SsgiResourceRole::DepthMip { mip: 1 },
    SsgiResourceRole::NormalMip { mip: 1 },
    SsgiResourceRole::DiffuseMip { mip: 1 },
];
const UPSAMPLE_1_TO_0_READS: [SsgiResourceRole; 6] = [
    SsgiResourceRole::DepthMip { mip: 1 },
    SsgiResourceRole::NormalMip { mip: 1 },
    SsgiResourceRole::FilteredDiffuseMip { mip: 1 },
    SsgiResourceRole::DepthMip { mip: 0 },
    SsgiResourceRole::NormalMip { mip: 0 },
    SsgiResourceRole::DiffuseMip { mip: 0 },
];
const UPSAMPLE_3_TO_2_WRITES: [SsgiResourceRole; 1] =
    [SsgiResourceRole::FilteredDiffuseMip { mip: 2 }];
const UPSAMPLE_2_TO_1_WRITES: [SsgiResourceRole; 1] =
    [SsgiResourceRole::FilteredDiffuseMip { mip: 1 }];
const UPSAMPLE_1_TO_0_WRITES: [SsgiResourceRole; 1] =
    [SsgiResourceRole::FilteredDiffuseMip { mip: 0 }];

pub(crate) const SSGI_PASS_DESCRIPTORS: [SsgiPassDescriptor; 13] = [
    deinterleave_descriptor!("ssgi_compute_deinterleave_2x", 0, DEINTERLEAVE_0_WRITES),
    deinterleave_descriptor!("ssgi_compute_deinterleave_4x", 1, DEINTERLEAVE_1_WRITES),
    deinterleave_descriptor!("ssgi_compute_deinterleave_8x", 2, DEINTERLEAVE_2_WRITES),
    deinterleave_descriptor!("ssgi_compute_deinterleave_16x", 3, DEINTERLEAVE_3_WRITES),
    diffuse_descriptor!(
        "ssgi_compute_diffuse_16x",
        3,
        DIFFUSE_3_READS,
        DIFFUSE_3_WRITES
    ),
    diffuse_descriptor!(
        "ssgi_compute_diffuse_8x",
        2,
        DIFFUSE_2_READS,
        DIFFUSE_2_WRITES
    ),
    diffuse_descriptor!(
        "ssgi_compute_diffuse_4x",
        1,
        DIFFUSE_1_READS,
        DIFFUSE_1_WRITES
    ),
    diffuse_descriptor!(
        "ssgi_compute_diffuse_2x",
        0,
        DIFFUSE_0_READS,
        DIFFUSE_0_WRITES
    ),
    upsample_descriptor!(
        "ssgi_compute_upsample_16x_to_8x",
        3,
        2,
        0,
        UPSAMPLE_3_TO_2_READS,
        UPSAMPLE_3_TO_2_WRITES
    ),
    upsample_descriptor!(
        "ssgi_compute_upsample_8x_to_4x",
        2,
        1,
        1,
        UPSAMPLE_2_TO_1_READS,
        UPSAMPLE_2_TO_1_WRITES
    ),
    upsample_descriptor!(
        "ssgi_compute_upsample_4x_to_2x",
        1,
        0,
        2,
        UPSAMPLE_1_TO_0_READS,
        UPSAMPLE_1_TO_0_WRITES
    ),
    SsgiPassDescriptor {
        name: SSGI_FINAL_PASS,
        kind: SsgiPassKind::FinalComposite,
        reads: &FINAL_READS,
        writes: &FINAL_WRITES,
        bindings: &FINAL_BINDINGS,
        shader: SsgiShaderStage::FinalFullscreen,
        dispatch: SsgiDispatchRule::Fullscreen,
        flags: PassFlags::empty(),
    },
    SsgiPassDescriptor {
        name: SSGI_COMPOSITE_PASS,
        kind: SsgiPassKind::SceneComposite,
        reads: &COMPOSITE_READS,
        writes: &COMPOSITE_WRITES,
        bindings: &COMPOSITE_BINDINGS,
        shader: SsgiShaderStage::CompositeFullscreen,
        dispatch: SsgiDispatchRule::Fullscreen,
        flags: PassFlags::empty(),
    },
];

#[inline]
pub(crate) fn ssgi_pass_descriptors() -> &'static [SsgiPassDescriptor] {
    &SSGI_PASS_DESCRIPTORS
}

#[inline]
pub(crate) fn ssgi_pass_descriptor(name: &str) -> Option<&'static SsgiPassDescriptor> {
    SSGI_PASS_DESCRIPTORS
        .iter()
        .find(|descriptor| descriptor.name == name)
}
