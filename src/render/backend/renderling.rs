use std::sync::Arc;

use rustc_hash::FxHashMap;
use winit::window::Window;

use crate::asset::{AssetId, Assets, Handle};
use crate::ecs::World;
use crate::math::Projection;
use crate::render::asset::{
    MeshAsset, MeshIndexData, MeshVertexAttribute, MeshVertexFormat, MeshVertexSemantic,
    StandardMaterialAsset,
};
use crate::render::pipeline::{RenderBackendKind, RenderPipelineAsset};
use crate::render::view::RenderStats;

use super::scene_renderer::{SceneRenderer, SceneRendererError, SceneRendererInitError};
use super::{SceneCamera, SceneSnapshot, SceneSnapshotExtractor, SceneSnapshotStats};

pub type RenderlingSceneSyncStats = SceneSnapshotStats;

type RCamera = ::renderling::slab::Hybrid<::renderling::camera::Camera>;
type RMaterial = ::renderling::slab::Hybrid<::renderling::pbr::Material>;
type RTransform = ::renderling::slab::Hybrid<::renderling::transform::Transform>;
type RRenderlet = ::renderling::slab::Hybrid<::renderling::stage::Renderlet>;
type RLight = ::renderling::slab::Hybrid<::renderling::pbr::light::Light>;
type RDirectionalLight = ::renderling::slab::Hybrid<::renderling::pbr::light::DirectionalLight>;
type RPointLight = ::renderling::slab::Hybrid<::renderling::pbr::light::PointLight>;

struct RenderlingMeshGpu {
    vertices: ::renderling::slab::HybridArray<::renderling::stage::Vertex>,
    indices: Option<::renderling::slab::HybridArray<u32>>,
}

pub struct RenderlingSceneRenderer {
    context: ::renderling::Context,
    stage: ::renderling::stage::Stage,
    surface_size: [u32; 2],
    stats: RenderStats,
    snapshot_extractor: SceneSnapshotExtractor,
    snapshot: SceneSnapshot,
    mesh_cache: FxHashMap<AssetId, RenderlingMeshGpu>,
    material_cache: FxHashMap<AssetId, RMaterial>,
    active_cameras: Vec<RCamera>,
    active_transforms: Vec<RTransform>,
    active_renderlets: Vec<RRenderlet>,
    active_lights: Vec<RLight>,
    active_directional_lights: Vec<RDirectionalLight>,
    active_point_lights: Vec<RPointLight>,
    warned_assets: bool,
}

impl RenderlingSceneRenderer {
    pub fn try_new(
        window: Arc<Window>,
        _vsync: bool,
        pipeline: RenderPipelineAsset,
    ) -> Result<Self, SceneRendererInitError> {
        debug_assert_eq!(pipeline.backend_kind(), RenderBackendKind::Renderling);
        let size = window.inner_size();
        let surface_size = [size.width.max(1), size.height.max(1)];
        let context = ::renderling::Context::try_from_raw_window_handle(
            window,
            surface_size[0],
            surface_size[1],
            None,
        )
        .map_err(|error| SceneRendererInitError::Other(error.to_string()))?;
        let stage = context
            .new_stage()
            .with_background_color([0.02, 0.025, 0.03, 1.0])
            .with_lighting(true);
        Ok(Self {
            context,
            stage,
            surface_size,
            stats: RenderStats::default(),
            snapshot_extractor: SceneSnapshotExtractor::new(),
            snapshot: SceneSnapshot::default(),
            mesh_cache: FxHashMap::default(),
            material_cache: FxHashMap::default(),
            active_cameras: Vec::new(),
            active_transforms: Vec::new(),
            active_renderlets: Vec::new(),
            active_lights: Vec::new(),
            active_directional_lights: Vec::new(),
            active_point_lights: Vec::new(),
            warned_assets: false,
        })
    }

    #[cfg(test)]
    fn new_with_headless_size(surface_size: [u32; 2]) -> Self {
        let context = ::renderling::Context::headless(surface_size[0], surface_size[1]);
        let stage = context
            .new_stage()
            .with_background_color([0.02, 0.025, 0.03, 1.0])
            .with_lighting(true);
        Self {
            context,
            stage,
            surface_size,
            stats: RenderStats::default(),
            snapshot_extractor: SceneSnapshotExtractor::new(),
            snapshot: SceneSnapshot::default(),
            mesh_cache: FxHashMap::default(),
            material_cache: FxHashMap::default(),
            active_cameras: Vec::new(),
            active_transforms: Vec::new(),
            active_renderlets: Vec::new(),
            active_lights: Vec::new(),
            active_directional_lights: Vec::new(),
            active_point_lights: Vec::new(),
            warned_assets: false,
        }
    }

    #[cfg(test)]
    pub(crate) fn new_for_tests(surface_size: [u32; 2]) -> Self {
        Self::new_with_headless_size(surface_size)
    }

    #[inline]
    pub fn snapshot(&self) -> &SceneSnapshot {
        &self.snapshot
    }

    #[inline]
    pub fn sync_stats(&self) -> RenderlingSceneSyncStats {
        self.snapshot.stats()
    }

    fn sync_world(&mut self, world: &World) {
        self.snapshot_extractor
            .extract_into(world, &mut self.snapshot);
        let stats = self.snapshot.stats();
        self.stats.view_count = stats.cameras;
        self.stats.light_count = stats.directional_lights + stats.point_lights + stats.spot_lights;
        self.stats.draw_calls = stats.mesh_instances;
        self.stats.resident_render_assets = self.mesh_cache.len() + self.material_cache.len();
    }

    fn rebuild_stage_frame(&mut self, assets: Option<&Assets>) {
        self.active_cameras.clear();
        self.active_transforms.clear();
        self.active_renderlets.clear();
        self.active_lights.clear();
        self.active_directional_lights.clear();
        self.active_point_lights.clear();

        let Some(assets) = assets else {
            if !self.warned_assets && self.snapshot.stats().mesh_instances > 0 {
                eprintln!(
                    "[SkyEngine] Renderling renderer cannot sync MeshAsset handles without an Assets resource"
                );
                self.warned_assets = true;
            }
            return;
        };
        self.warned_assets = false;

        let camera = self
            .snapshot
            .cameras()
            .next()
            .map(|(_, camera)| camera)
            .copied();
        let camera = self.stage.new_value(renderling_camera(
            camera,
            self.surface_size[0],
            self.surface_size[1],
        ));
        let camera_id = camera.id();
        self.active_cameras.push(camera);

        let mut light_ids = Vec::new();
        for (_, light) in self.snapshot.directional_lights() {
            let directional = self
                .stage
                .new_value(::renderling::pbr::light::DirectionalLight {
                    direction: rvec3(light.direction),
                    color: rvec4(light.color),
                    intensity: light.intensity.max(0.0),
                });
            let light = self
                .stage
                .new_value(::renderling::pbr::light::Light::from(directional.id()));
            light_ids.push(light.id());
            self.active_directional_lights.push(directional);
            self.active_lights.push(light);
        }
        for (_, light) in self.snapshot.point_lights() {
            let point = self.stage.new_value(::renderling::pbr::light::PointLight {
                position: rvec3(light.position),
                color: rvec4(light.color),
                intensity: light.intensity.max(0.0),
            });
            let light = self
                .stage
                .new_value(::renderling::pbr::light::Light::from(point.id()));
            light_ids.push(light.id());
            self.active_point_lights.push(point);
            self.active_lights.push(light);
        }
        self.stage.set_lights(light_ids);

        let mesh_instances = self
            .snapshot
            .mesh_instances()
            .map(|(entity, instance)| (entity, instance.clone()))
            .collect::<Vec<_>>();

        for (entity, instance) in mesh_instances {
            let Some((vertices_array, indices_array)) =
                self.sync_mesh_arrays(assets, instance.mesh)
            else {
                continue;
            };
            let material_id = instance
                .materials
                .first()
                .and_then(|id| {
                    self.sync_material(assets, *id)
                        .map(|material| material.id())
                })
                .unwrap_or_default();
            let transform = self
                .stage
                .new_value(renderling_transform(instance.transform));
            let renderlet = self.stage.new_value(::renderling::stage::Renderlet {
                camera_id,
                vertices_array,
                indices_array,
                material_id,
                transform_id: transform.id(),
                ..Default::default()
            });
            self.stage.add_renderlet(&renderlet);
            self.active_transforms.push(transform);
            self.active_renderlets.push(renderlet);
            let _ = entity;
        }
    }

    fn sync_mesh_arrays(
        &mut self,
        assets: &Assets,
        mesh_id: AssetId,
    ) -> Option<(
        ::renderling::slab::Array<::renderling::stage::Vertex>,
        ::renderling::slab::Array<u32>,
    )> {
        let mesh_gpu = self.sync_mesh(assets, mesh_id)?;
        Some((
            mesh_gpu.vertices.array(),
            mesh_gpu
                .indices
                .as_ref()
                .map(|indices| indices.array())
                .unwrap_or_default(),
        ))
    }

    fn sync_mesh(&mut self, assets: &Assets, mesh_id: AssetId) -> Option<&RenderlingMeshGpu> {
        if !self.mesh_cache.contains_key(&mesh_id) {
            let mesh = assets.try_get_id::<MeshAsset>(mesh_id)?;
            match build_renderling_mesh(&self.stage, &mesh) {
                Some(gpu) => {
                    self.mesh_cache.insert(mesh_id, gpu);
                    self.stats.uploaded_render_assets =
                        self.stats.uploaded_render_assets.saturating_add(1);
                }
                None => return None,
            }
        }
        self.mesh_cache.get(&mesh_id)
    }

    fn sync_material(&mut self, assets: &Assets, material_id: AssetId) -> Option<&RMaterial> {
        if !self.material_cache.contains_key(&material_id) {
            let material = assets.try_get_id::<StandardMaterialAsset>(material_id);
            let material = self
                .stage
                .new_value(renderling_material(material.as_deref()));
            self.material_cache.insert(material_id, material);
            self.stats.uploaded_render_assets = self.stats.uploaded_render_assets.saturating_add(1);
        }
        self.material_cache.get(&material_id)
    }
}

impl SceneRenderer for RenderlingSceneRenderer {
    fn backend_kind(&self) -> RenderBackendKind {
        RenderBackendKind::Renderling
    }

    fn begin_frame(&mut self) -> Result<(), SceneRendererError> {
        Ok(())
    }

    fn end_frame(&mut self) {}

    fn render_world(&mut self, world: &World) {
        self.stats.uploaded_render_assets = 0;
        self.sync_world(world);
        let snapshot = self.snapshot.clone();
        let assets = world.get_resource::<Assets>().cloned();
        self.snapshot = snapshot;
        self.rebuild_stage_frame(assets.as_ref());

        match self.context.get_next_frame() {
            Ok(frame) => {
                self.stage.render(&frame.view());
                frame.present();
            }
            Err(error) => {
                eprintln!("[SkyEngine] Renderling frame failed: {error}");
            }
        }
    }

    fn resize(&mut self, width: u32, height: u32) {
        self.surface_size = [width.max(1), height.max(1)];
        let size =
            ::renderling::prelude::glam::UVec2::new(self.surface_size[0], self.surface_size[1]);
        self.context.set_size(size);
        self.stage.set_size(size);
    }

    fn surface_lost(&mut self) {}

    fn stats(&self) -> RenderStats {
        self.stats
    }

    fn surface_size(&self) -> [u32; 2] {
        self.surface_size
    }

    fn adapter_name(&self) -> &str {
        "Renderling"
    }

    fn backend_name(&self) -> &str {
        "wgpu"
    }
}

fn build_renderling_mesh(
    stage: &::renderling::stage::Stage,
    mesh: &MeshAsset,
) -> Option<RenderlingMeshGpu> {
    let position = optional_attribute(mesh, MeshVertexSemantic::Position)?;
    let normal = optional_attribute(mesh, MeshVertexSemantic::Normal);
    let tangent = optional_attribute(mesh, MeshVertexSemantic::Tangent);
    let uv0 = optional_attribute(mesh, MeshVertexSemantic::UV0);
    let uv1 = optional_attribute(mesh, MeshVertexSemantic::UV1);
    let color = optional_attribute(mesh, MeshVertexSemantic::Color);

    let mut vertices = Vec::with_capacity(mesh.vertex_count() as usize);
    for vertex in 0..mesh.vertex_count() as usize {
        let position = read_vec3(mesh, vertex, position)?;
        let normal = normal
            .and_then(|attribute| read_vec3(mesh, vertex, attribute))
            .unwrap_or([0.0, 1.0, 0.0]);
        let tangent = tangent
            .and_then(|attribute| read_vec4(mesh, vertex, attribute))
            .unwrap_or([1.0, 0.0, 0.0, 1.0]);
        let uv0 = uv0
            .and_then(|attribute| read_vec2(mesh, vertex, attribute))
            .unwrap_or([0.0, 0.0]);
        let uv1 = uv1
            .and_then(|attribute| read_vec2(mesh, vertex, attribute))
            .unwrap_or([0.0, 0.0]);
        let color = color
            .and_then(|attribute| read_color(mesh, vertex, attribute))
            .unwrap_or([1.0, 1.0, 1.0, 1.0]);

        vertices.push(::renderling::stage::Vertex {
            position: rvec3(position),
            normal: rvec3(normal),
            tangent: rvec4(tangent),
            uv0: rvec2(uv0),
            uv1: rvec2(uv1),
            color: rvec4(color),
            ..Default::default()
        });
    }

    let vertices = stage.new_array(vertices);
    let indices = mesh_indices(mesh).map(|indices| stage.new_array(indices));
    Some(RenderlingMeshGpu { vertices, indices })
}

fn renderling_material(material: Option<&StandardMaterialAsset>) -> ::renderling::pbr::Material {
    let mut out = ::renderling::pbr::Material::default();
    if let Some(material) = material {
        out.albedo_factor = rvec4(material.albedo.to_array());
        out.metallic_factor = material.metallic;
        out.roughness_factor = material.roughness;
        out.emissive_factor = rvec3([
            material.emissive.r,
            material.emissive.g,
            material.emissive.b,
        ]);
        out.has_lighting = true;
    }
    out
}

fn renderling_camera(
    camera: Option<SceneCamera>,
    width: u32,
    height: u32,
) -> ::renderling::camera::Camera {
    let Some(camera) = camera else {
        let (projection, view) =
            ::renderling::camera::default_perspective(width.max(1) as f32, height.max(1) as f32);
        return ::renderling::camera::Camera::new(projection, view);
    };

    let view_to_world = ::renderling::prelude::glam::Mat4::from_cols_array(&camera.transform);
    let world_to_view = view_to_world.inverse();
    let projection = match camera.projection {
        Projection::Perspective {
            vertical_fov_radians,
            near,
            far,
            ..
        } => ::renderling::prelude::glam::Mat4::perspective_rh(
            vertical_fov_radians,
            width.max(1) as f32 / height.max(1) as f32,
            near,
            far,
        ),
        Projection::Orthographic { .. } | Projection::OrthographicFixed { .. } => {
            ::renderling::prelude::glam::Mat4::from_cols_array(
                &camera
                    .projection
                    .projection_matrix(crate::math::Vec2::new(width as f32, height as f32))
                    .to_cols_array(),
            )
        }
    };

    ::renderling::camera::Camera {
        projection,
        view: world_to_view,
        position: view_to_world.transform_point3(::renderling::prelude::glam::Vec3::ZERO),
    }
}

fn renderling_transform(cols: [f32; 16]) -> ::renderling::transform::Transform {
    let matrix = ::renderling::prelude::glam::Mat4::from_cols_array(&cols);
    let (scale, rotation, translation) = matrix.to_scale_rotation_translation();
    ::renderling::transform::Transform {
        translation,
        rotation,
        scale,
    }
}

fn optional_attribute(
    mesh: &MeshAsset,
    semantic: MeshVertexSemantic,
) -> Option<MeshVertexAttribute> {
    mesh.vertex_layout()
        .attributes()
        .iter()
        .copied()
        .find(|attribute| attribute.semantic == semantic)
}

fn mesh_indices(mesh: &MeshAsset) -> Option<Vec<u32>> {
    match mesh.indices() {
        Some(MeshIndexData::U16(indices)) => {
            Some(indices.iter().map(|index| *index as u32).collect())
        }
        Some(MeshIndexData::U32(indices)) => Some(indices.iter().copied().collect()),
        None => None,
    }
}

fn read_vec2(mesh: &MeshAsset, vertex: usize, attribute: MeshVertexAttribute) -> Option<[f32; 2]> {
    (attribute.format == MeshVertexFormat::Float32x2)
        .then(|| read_f32s::<2>(mesh, vertex, attribute.offset))
        .flatten()
}

fn read_vec3(mesh: &MeshAsset, vertex: usize, attribute: MeshVertexAttribute) -> Option<[f32; 3]> {
    (attribute.format == MeshVertexFormat::Float32x3)
        .then(|| read_f32s::<3>(mesh, vertex, attribute.offset))
        .flatten()
}

fn read_vec4(mesh: &MeshAsset, vertex: usize, attribute: MeshVertexAttribute) -> Option<[f32; 4]> {
    (attribute.format == MeshVertexFormat::Float32x4)
        .then(|| read_f32s::<4>(mesh, vertex, attribute.offset))
        .flatten()
}

fn read_color(mesh: &MeshAsset, vertex: usize, attribute: MeshVertexAttribute) -> Option<[f32; 4]> {
    match attribute.format {
        MeshVertexFormat::Float32x4 => read_f32s::<4>(mesh, vertex, attribute.offset),
        MeshVertexFormat::Unorm8x4 => {
            let bytes = read_bytes(mesh, vertex, attribute.offset, 4)?;
            Some([
                bytes[0] as f32 / 255.0,
                bytes[1] as f32 / 255.0,
                bytes[2] as f32 / 255.0,
                bytes[3] as f32 / 255.0,
            ])
        }
        _ => None,
    }
}

fn read_f32s<const N: usize>(mesh: &MeshAsset, vertex: usize, offset: u32) -> Option<[f32; N]> {
    let bytes = read_bytes(mesh, vertex, offset, N * std::mem::size_of::<f32>())?;
    let mut values = [0.0; N];
    for (index, chunk) in bytes.chunks_exact(4).enumerate() {
        values[index] = f32::from_ne_bytes(chunk.try_into().ok()?);
    }
    Some(values)
}

fn read_bytes(mesh: &MeshAsset, vertex: usize, offset: u32, len: usize) -> Option<&[u8]> {
    let base = vertex
        .checked_mul(mesh.vertex_layout().stride() as usize)?
        .checked_add(offset as usize)?;
    mesh.vertex_bytes().get(base..base.checked_add(len)?)
}

#[inline]
fn rvec2(value: [f32; 2]) -> ::renderling::prelude::glam::Vec2 {
    ::renderling::prelude::glam::Vec2::new(value[0], value[1])
}

#[inline]
fn rvec3(value: [f32; 3]) -> ::renderling::prelude::glam::Vec3 {
    ::renderling::prelude::glam::Vec3::new(value[0], value[1], value[2])
}

#[inline]
fn rvec4(value: [f32; 4]) -> ::renderling::prelude::glam::Vec4 {
    ::renderling::prelude::glam::Vec4::new(value[0], value[1], value[2], value[3])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset::{AssetConfig, Assets};
    use crate::ecs::World;
    use crate::render::{
        DirectionalLight, MeshAssetDescriptor, MeshRenderer, MeshVertexLayout, Transform,
    };

    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct Vertex {
        position: [f32; 3],
        normal: [f32; 3],
        uv: [f32; 2],
    }

    #[test]
    fn renderling_scene_renderer_uses_neutral_scene_snapshot() {
        let mut world = World::new();
        let assets = Assets::with_empty_manifest(AssetConfig::default());
        let mesh = assets.insert_runtime(MeshAsset::from_raw(MeshAssetDescriptor::new(
            bytemuck::cast_slice(&[
                Vertex {
                    position: [0.0, 0.0, 0.0],
                    normal: [0.0, 1.0, 0.0],
                    uv: [0.0, 0.0],
                },
                Vertex {
                    position: [1.0, 0.0, 0.0],
                    normal: [0.0, 1.0, 0.0],
                    uv: [1.0, 0.0],
                },
                Vertex {
                    position: [0.0, 1.0, 0.0],
                    normal: [0.0, 1.0, 0.0],
                    uv: [0.0, 1.0],
                },
            ]),
            3,
            MeshVertexLayout::position_normal_uv(),
            "renderling_snapshot_triangle",
        )));
        let material = assets.insert_runtime(StandardMaterialAsset::new());
        world.insert_resource(assets);

        let mesh_entity = world.spawn((Transform::default(), MeshRenderer::new(mesh, material)));
        let _light = world.spawn((Transform::default(), DirectionalLight::default()));
        let _camera = world.spawn((
            Transform::default(),
            crate::render::CameraMarker::new(),
            Projection::perspective(60.0f32.to_radians(), 0.1, 100.0),
        ));

        let mut renderer = RenderlingSceneRenderer::new_for_tests([128, 96]);
        renderer.render_world(&world);
        assert_eq!(
            renderer.sync_stats(),
            RenderlingSceneSyncStats {
                mesh_instances: 1,
                directional_lights: 1,
                point_lights: 0,
                spot_lights: 0,
                cameras: 1,
            }
        );
        assert!(renderer.snapshot().mesh_instance(mesh_entity).is_some());
    }
}
