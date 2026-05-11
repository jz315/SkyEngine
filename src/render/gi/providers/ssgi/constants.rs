use crate::render::gi::GiProviderId;

pub const SSGI_PROVIDER_ID: GiProviderId = "sky.ssgi";

pub(crate) const SSGI_FINAL_SHADER: &str = include_str!("../../../shaders/gi/ssgi_final.wgsl");
pub(crate) const SSGI_COMPOSITE_SHADER: &str =
    include_str!("../../../shaders/gi/ssgi_composite.wgsl");
pub(crate) const SSGI_DEINTERLEAVE_COMPUTE_SHADER: &str =
    include_str!("../../../shaders/gi/ssgi_deinterleave_compute.wgsl");
pub(crate) const SSGI_COMPUTE_SHADER: &str = include_str!("../../../shaders/gi/ssgi_compute.wgsl");
pub(crate) const SSGI_UPSAMPLE_COMPUTE_SHADER: &str =
    include_str!("../../../shaders/gi/ssgi_upsample_compute.wgsl");

pub(crate) const SSGI_MIP_COUNT: usize = 4;
pub(crate) const SSGI_ATLAS_LAYERS: u32 = 16;
pub(crate) const SSGI_INTERNAL_ALIGNMENT: u32 = 64;

pub(crate) const SSGI_COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
pub(crate) const SSGI_DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R32Float;
pub(crate) const SSGI_NORMAL_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
pub(crate) const SSGI_COMPUTE_TEXTURE_USAGE: wgpu::TextureUsages =
    wgpu::TextureUsages::TEXTURE_BINDING
        .union(wgpu::TextureUsages::STORAGE_BINDING)
        .union(wgpu::TextureUsages::COPY_SRC)
        .union(wgpu::TextureUsages::COPY_DST);

pub(crate) const SSGI_GRAPH_RESOURCES_BLACKBOARD: &str = "ssgi_graph_resources";
pub(crate) const SSGI_TEXTURE_ATLAS_COLOR: &str = "ssgi_texture_atlas_color";
pub(crate) const SSGI_TEXTURE_ATLAS_DEPTH: &str = "ssgi_texture_atlas_depth";
pub(crate) const SSGI_TEXTURE_DEPTH_MIPS: &str = "ssgi_texture_depth_mips";
pub(crate) const SSGI_TEXTURE_NORMAL_MIPS: &str = "ssgi_texture_normal_mips";
pub(crate) const SSGI_TEXTURE_DIFFUSE_MIPS: &str = "ssgi_texture_diffuse_mips";
pub(crate) const SSGI_TEXTURE_FILTERED_DIFFUSE_MIPS: &str = "ssgi_texture_filtered_diffuse_mips";
pub(crate) const SSGI_TEXTURE_INDIRECT_DIFFUSE: &str = "ssgi_texture_indirect_diffuse";
pub(crate) const SSGI_TEXTURE_SCENE_COLOR: &str = "ssgi_scene_color";
pub(crate) const SSGI_FINAL_PASS: &str = "ssgi_final_upsample";
pub(crate) const SSGI_COMPOSITE_PASS: &str = "ssgi_scene_composite";
