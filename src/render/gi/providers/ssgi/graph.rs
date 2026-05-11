use super::constants::{
    SSGI_ATLAS_LAYERS, SSGI_GRAPH_RESOURCES_BLACKBOARD, SSGI_TEXTURE_INDIRECT_DIFFUSE,
    SSGI_TEXTURE_SCENE_COLOR,
};
use super::contract::{ssgi_pass_descriptors, SsgiPassDescriptor, SsgiPassKind, SsgiResourceRole};
use super::layout::{ssgi_compute_texture_specs, SsgiResources};
use crate::render::graph::{
    CompiledPass, PassSetup, RenderGraph, RenderGraphError, ResourceRef, TargetSize, TextureHandle,
    TextureSubresource,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SsgiGraphInputs {
    pub(crate) scene_color: TextureHandle,
    pub(crate) scene_depth: TextureHandle,
    pub(crate) scene_normal: TextureHandle,
    pub(crate) scene_velocity: TextureHandle,
    pub(crate) target_size: [u32; 2],
    pub(crate) output_format: wgpu::TextureFormat,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SsgiGraphResources {
    pub(crate) scene_color: TextureHandle,
    pub(crate) scene_depth: TextureHandle,
    pub(crate) scene_normal: TextureHandle,
    pub(crate) scene_velocity: TextureHandle,
    pub(crate) output_indirect_diffuse: TextureHandle,
    pub(crate) output_scene_color: TextureHandle,
    pub(crate) atlas_color: TextureHandle,
    pub(crate) atlas_depth: TextureHandle,
    pub(crate) depth_mips: TextureHandle,
    pub(crate) normal_mips: TextureHandle,
    pub(crate) diffuse_mips: TextureHandle,
    pub(crate) filtered_diffuse_mips: TextureHandle,
}

impl SsgiGraphResources {
    #[inline]
    pub(crate) fn output_scene_color(self) -> TextureHandle {
        self.output_scene_color
    }

    #[inline]
    pub(crate) fn output_indirect_diffuse(self) -> TextureHandle {
        self.output_indirect_diffuse
    }

    #[inline]
    #[cfg(test)]
    pub(crate) fn atlas_color_layer(self, mip_level: u32, layer: u32) -> TextureSubresource {
        TextureSubresource::new(self.atlas_color, mip_level, 1, layer, 1)
    }

    #[inline]
    #[cfg(test)]
    pub(crate) fn atlas_depth_layer(self, mip_level: u32, layer: u32) -> TextureSubresource {
        TextureSubresource::new(self.atlas_depth, mip_level, 1, layer, 1)
    }

    #[inline]
    pub(crate) fn depth_mip(self, mip_level: u32) -> TextureSubresource {
        TextureSubresource::new(self.depth_mips, mip_level, 1, 0, 1)
    }

    #[inline]
    pub(crate) fn normal_mip(self, mip_level: u32) -> TextureSubresource {
        TextureSubresource::new(self.normal_mips, mip_level, 1, 0, 1)
    }

    #[inline]
    pub(crate) fn diffuse_mip(self, mip_level: u32) -> TextureSubresource {
        TextureSubresource::new(self.diffuse_mips, mip_level, 1, 0, 1)
    }

    #[inline]
    pub(crate) fn filtered_diffuse_mip(self, mip_level: u32) -> TextureSubresource {
        TextureSubresource::new(self.filtered_diffuse_mips, mip_level, 1, 0, 1)
    }

    pub(crate) fn resolve(self, role: SsgiResourceRole) -> ResourceRef {
        match role {
            SsgiResourceRole::SceneColorInput => ResourceRef::Texture(self.scene_color),
            SsgiResourceRole::SceneDepth => ResourceRef::Texture(self.scene_depth),
            SsgiResourceRole::SceneNormal => ResourceRef::Texture(self.scene_normal),
            SsgiResourceRole::SceneVelocity => ResourceRef::Texture(self.scene_velocity),
            SsgiResourceRole::AtlasDepth { mip } => ResourceRef::TextureSubresource(
                TextureSubresource::new(self.atlas_depth, mip, 1, 0, SSGI_ATLAS_LAYERS),
            ),
            SsgiResourceRole::AtlasColor { mip } => ResourceRef::TextureSubresource(
                TextureSubresource::new(self.atlas_color, mip, 1, 0, SSGI_ATLAS_LAYERS),
            ),
            SsgiResourceRole::DepthMip { mip } => {
                ResourceRef::TextureSubresource(self.depth_mip(mip))
            }
            SsgiResourceRole::NormalMip { mip } => {
                ResourceRef::TextureSubresource(self.normal_mip(mip))
            }
            SsgiResourceRole::DiffuseMip { mip } => {
                ResourceRef::TextureSubresource(self.diffuse_mip(mip))
            }
            SsgiResourceRole::FilteredDiffuseMip { mip } => {
                ResourceRef::TextureSubresource(self.filtered_diffuse_mip(mip))
            }
            SsgiResourceRole::FinalIndirectDiffuse => {
                ResourceRef::Texture(self.output_indirect_diffuse)
            }
            SsgiResourceRole::OutputSceneColor => ResourceRef::Texture(self.output_scene_color),
        }
    }
}

pub(crate) struct SsgiResolvedPassResources {
    descriptor: &'static SsgiPassDescriptor,
    resources: SsgiGraphResources,
}

impl SsgiResolvedPassResources {
    pub(crate) fn read_texture(
        &self,
        role: SsgiResourceRole,
    ) -> Result<TextureHandle, RenderGraphError> {
        if !self.descriptor.reads.contains(&role) {
            return Err(role_error(
                self.descriptor.name,
                "read",
                role,
                "is not declared",
            ));
        }
        match self.resources.resolve(role) {
            ResourceRef::Texture(handle) => Ok(handle),
            resource => Err(role_type_error(
                self.descriptor.name,
                role,
                "texture",
                resource,
            )),
        }
    }

    pub(crate) fn read_subresource(
        &self,
        role: SsgiResourceRole,
    ) -> Result<TextureSubresource, RenderGraphError> {
        if !self.descriptor.reads.contains(&role) {
            return Err(role_error(
                self.descriptor.name,
                "read",
                role,
                "is not declared",
            ));
        }
        match self.resources.resolve(role) {
            ResourceRef::TextureSubresource(subresource) => Ok(subresource),
            resource => Err(role_type_error(
                self.descriptor.name,
                role,
                "texture subresource",
                resource,
            )),
        }
    }

    pub(crate) fn write_texture(
        &self,
        role: SsgiResourceRole,
    ) -> Result<TextureHandle, RenderGraphError> {
        if !self.descriptor.writes.contains(&role) {
            return Err(role_error(
                self.descriptor.name,
                "write",
                role,
                "is not declared",
            ));
        }
        match self.resources.resolve(role) {
            ResourceRef::Texture(handle) => Ok(handle),
            resource => Err(role_type_error(
                self.descriptor.name,
                role,
                "texture",
                resource,
            )),
        }
    }

    pub(crate) fn write_subresource(
        &self,
        role: SsgiResourceRole,
    ) -> Result<TextureSubresource, RenderGraphError> {
        if !self.descriptor.writes.contains(&role) {
            return Err(role_error(
                self.descriptor.name,
                "write",
                role,
                "is not declared",
            ));
        }
        match self.resources.resolve(role) {
            ResourceRef::TextureSubresource(subresource) => Ok(subresource),
            resource => Err(role_type_error(
                self.descriptor.name,
                role,
                "texture subresource",
                resource,
            )),
        }
    }
}

pub(crate) fn declare_ssgi_graph(
    graph: &mut RenderGraph,
    resources: SsgiResources,
    inputs: SsgiGraphInputs,
) -> SsgiGraphResources {
    let specs = ssgi_compute_texture_specs(resources);
    let atlas_color = specs.atlas_color.create(graph);
    let atlas_depth = specs.atlas_depth.create(graph);
    let depth_mips = specs.depth_mips.create(graph);
    let normal_mips = specs.normal_mips.create(graph);
    let diffuse_mips = specs.diffuse_mips.create(graph);
    let filtered_diffuse_mips = specs.filtered_diffuse_mips.create(graph);
    let output_indirect_diffuse = graph.create_texture(|builder| {
        builder
            .name(SSGI_TEXTURE_INDIRECT_DIFFUSE)
            .size(TargetSize::Exact(
                inputs.target_size[0],
                inputs.target_size[1],
            ))
            .format(inputs.output_format);
    });
    let output_scene_color = graph.create_texture(|builder| {
        builder
            .name(SSGI_TEXTURE_SCENE_COLOR)
            .size(TargetSize::Exact(
                inputs.target_size[0],
                inputs.target_size[1],
            ))
            .format(inputs.output_format);
    });

    let graph_resources = SsgiGraphResources {
        scene_color: inputs.scene_color,
        scene_depth: inputs.scene_depth,
        scene_normal: inputs.scene_normal,
        scene_velocity: inputs.scene_velocity,
        output_indirect_diffuse,
        output_scene_color,
        atlas_color,
        atlas_depth,
        depth_mips,
        normal_mips,
        diffuse_mips,
        filtered_diffuse_mips,
    };

    for descriptor in ssgi_pass_descriptors() {
        match descriptor.kind {
            SsgiPassKind::FinalComposite | SsgiPassKind::SceneComposite => {
                graph.add_render_pass(descriptor.name, |setup| {
                    declare_pass_resources(setup, descriptor, graph_resources);
                });
            }
            _ => {
                graph.add_compute_pass(descriptor.name, |setup| {
                    declare_pass_resources(setup, descriptor, graph_resources);
                });
            }
        }
    }

    graph_resources
}

pub(crate) fn validate_compiled_pass_resources(
    pass: &CompiledPass,
    descriptor: &'static SsgiPassDescriptor,
    graph_resources: SsgiGraphResources,
) -> Result<SsgiResolvedPassResources, RenderGraphError> {
    if pass.name.as_ref() != descriptor.name {
        return Err(RenderGraphError::ExecutionFailed(format!(
            "SSGI contract mismatch: compiled pass `{}` was validated against descriptor `{}`",
            pass.name, descriptor.name
        )));
    }

    validate_resource_set(
        descriptor.name,
        "read",
        &pass.reads,
        descriptor.reads,
        graph_resources,
    )?;
    validate_resource_set(
        descriptor.name,
        "write",
        &pass.writes,
        descriptor.writes,
        graph_resources,
    )?;

    Ok(SsgiResolvedPassResources {
        descriptor,
        resources: graph_resources,
    })
}

#[inline]
pub(crate) fn graph_resources_blackboard_key() -> &'static str {
    SSGI_GRAPH_RESOURCES_BLACKBOARD
}

fn declare_pass_resources(
    setup: &mut PassSetup,
    descriptor: &SsgiPassDescriptor,
    graph_resources: SsgiGraphResources,
) {
    for role in descriptor.reads {
        declare_read(setup, graph_resources.resolve(*role));
    }

    for role in descriptor.writes {
        if *role == SsgiResourceRole::FinalIndirectDiffuse {
            setup.write_color_cleared(
                0,
                graph_resources.output_indirect_diffuse(),
                [0.0, 0.0, 0.0, 1.0],
            );
        } else if *role == SsgiResourceRole::OutputSceneColor {
            setup.write_color_cleared(
                0,
                graph_resources.output_scene_color(),
                [0.0, 0.0, 0.0, 1.0],
            );
        } else {
            declare_write(setup, graph_resources.resolve(*role));
        }
    }

    setup.with_flags(descriptor.flags);
}

fn declare_read(setup: &mut PassSetup, resource: ResourceRef) {
    match resource {
        ResourceRef::Texture(handle) => setup.read(handle),
        ResourceRef::TextureSubresource(subresource) => setup.read_subresource(subresource),
        ResourceRef::Buffer(handle) => setup.read_buffer(handle),
        ResourceRef::Surface => {}
    }
}

fn declare_write(setup: &mut PassSetup, resource: ResourceRef) {
    match resource {
        ResourceRef::Texture(handle) => setup.write(handle),
        ResourceRef::TextureSubresource(subresource) => setup.write_subresource(subresource),
        ResourceRef::Buffer(handle) => setup.write_buffer(handle),
        ResourceRef::Surface => setup.write_surface(),
    }
}

fn validate_resource_set(
    pass_name: &str,
    access: &str,
    actual: &[ResourceRef],
    roles: &[SsgiResourceRole],
    graph_resources: SsgiGraphResources,
) -> Result<(), RenderGraphError> {
    let expected: Vec<ResourceRef> = roles
        .iter()
        .map(|role| graph_resources.resolve(*role))
        .collect();
    for (role, resource) in roles.iter().zip(expected.iter()) {
        if !actual.contains(resource) {
            return Err(RenderGraphError::ExecutionFailed(format!(
                "SSGI pass `{pass_name}` missing {access} role {role:?} mapped to {resource:?}"
            )));
        }
    }
    if actual.len() != expected.len() {
        return Err(RenderGraphError::ExecutionFailed(format!(
            "SSGI pass `{pass_name}` has unexpected {access} resource count: actual={}, expected={}",
            actual.len(),
            expected.len()
        )));
    }
    Ok(())
}

fn role_error(
    pass_name: &str,
    access: &str,
    role: SsgiResourceRole,
    message: &str,
) -> RenderGraphError {
    RenderGraphError::ExecutionFailed(format!(
        "SSGI pass `{pass_name}` {access} role {role:?} {message}"
    ))
}

fn role_type_error(
    pass_name: &str,
    role: SsgiResourceRole,
    expected: &str,
    actual: ResourceRef,
) -> RenderGraphError {
    RenderGraphError::ExecutionFailed(format!(
        "SSGI pass `{pass_name}` role {role:?} expected {expected}, got {actual:?}"
    ))
}
