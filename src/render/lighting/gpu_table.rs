use crate::gpu::GpuContext;
use crate::render::gpu::GpuTable;

const INITIAL_LIGHT_CAPACITY: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuLightKind {
    Point,
    Directional,
    Spot,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuLight {
    /// Point lights store world-space position.xyz and radius in w.
    /// Directional lights store normalized light direction.xyz and zero radius.
    pub pos_radius: [f32; 4],
    pub color: [f32; 4],
    /// x = falloff for point lights
    /// y = kind (0.0 = point, 1.0 = directional, 2.0 = spot)
    /// z = spot inner cone cosine
    /// w = spot outer cone cosine
    pub falloff: [f32; 4],
    /// Spot lights store normalized cone direction.xyz.
    /// w is reserved for a future shadow record index.
    pub dir_shadow: [f32; 4],
}

impl GpuLight {
    #[inline]
    pub fn kind(self) -> GpuLightKind {
        match self.falloff[1].round() as i32 {
            1 => GpuLightKind::Directional,
            2 => GpuLightKind::Spot,
            _ => GpuLightKind::Point,
        }
    }

    #[inline]
    pub fn is_point(self) -> bool {
        self.kind() == GpuLightKind::Point
    }

    #[inline]
    pub fn is_directional(self) -> bool {
        self.kind() == GpuLightKind::Directional
    }

    #[inline]
    pub fn is_spot(self) -> bool {
        self.kind() == GpuLightKind::Spot
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
struct LightTableMeta {
    count: u32,
    _pad: [u32; 3],
}

pub struct LightTable {
    buffer: wgpu::Buffer,
    meta_buffer: wgpu::Buffer,
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    lights: Vec<GpuLight>,
    capacity: usize,
    dirty: bool,
}

impl LightTable {
    pub fn new(ctx: &GpuContext) -> Self {
        let bind_group_layout =
            ctx.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("light_table_bgl"),
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: true },
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Uniform,
                                has_dynamic_offset: false,
                                min_binding_size: Some(
                                    std::num::NonZeroU64::new(
                                        std::mem::size_of::<LightTableMeta>() as u64,
                                    )
                                    .expect("LightTableMeta has non-zero size"),
                                ),
                            },
                            count: None,
                        },
                    ],
                });
        let buffer = create_light_buffer(ctx, INITIAL_LIGHT_CAPACITY);
        let meta_buffer = create_light_meta_buffer(ctx);
        let bind_group = create_light_bind_group(ctx, &bind_group_layout, &buffer, &meta_buffer);

        Self {
            buffer,
            meta_buffer,
            bind_group_layout,
            bind_group,
            lights: Vec::with_capacity(INITIAL_LIGHT_CAPACITY),
            capacity: INITIAL_LIGHT_CAPACITY,
            dirty: true,
        }
    }

    pub fn set_all(&mut self, ctx: &GpuContext, lights: &[GpuLight]) {
        self.ensure_capacity(ctx, lights.len().max(1));
        self.lights.clear();
        self.lights.extend_from_slice(lights);
        self.dirty = true;
    }

    #[inline]
    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }

    #[inline]
    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }

    #[inline]
    pub fn lights(&self) -> &[GpuLight] {
        &self.lights
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.lights.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.lights.is_empty()
    }

    #[inline]
    pub fn count_kind(&self, kind: GpuLightKind) -> usize {
        self.lights
            .iter()
            .filter(|light| light.kind() == kind)
            .count()
    }

    #[inline]
    pub fn point_count(&self) -> usize {
        self.count_kind(GpuLightKind::Point)
    }

    #[inline]
    pub fn directional_count(&self) -> usize {
        self.count_kind(GpuLightKind::Directional)
    }

    #[inline]
    pub fn spot_count(&self) -> usize {
        self.count_kind(GpuLightKind::Spot)
    }

    #[inline]
    pub(crate) fn buffer(&self) -> &wgpu::Buffer {
        &self.buffer
    }

    #[inline]
    pub(crate) fn meta_buffer(&self) -> &wgpu::Buffer {
        &self.meta_buffer
    }

    fn ensure_capacity(&mut self, ctx: &GpuContext, required: usize) {
        if required <= self.capacity {
            return;
        }

        self.capacity = required.next_power_of_two().max(INITIAL_LIGHT_CAPACITY);
        self.buffer = create_light_buffer(ctx, self.capacity);
        self.bind_group = create_light_bind_group(
            ctx,
            &self.bind_group_layout,
            &self.buffer,
            &self.meta_buffer,
        );
        self.dirty = true;
    }

    pub(crate) fn upload(&mut self, queue: &wgpu::Queue) {
        if !self.dirty {
            return;
        }

        if !self.lights.is_empty() {
            queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&self.lights));
        }
        let meta = LightTableMeta {
            count: self.lights.len() as u32,
            _pad: [0; 3],
        };
        queue.write_buffer(&self.meta_buffer, 0, bytemuck::bytes_of(&meta));
        self.dirty = false;
    }
}

impl GpuTable for LightTable {
    fn name(&self) -> &'static str {
        "lights"
    }

    fn upload(&mut self, queue: &wgpu::Queue) {
        self.upload(queue);
    }

    fn bind_group(&self) -> &wgpu::BindGroup {
        self.bind_group()
    }

    fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        self.bind_group_layout()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[derive(Clone, Copy)]
pub struct SceneLightingResources<'a> {
    light_table: &'a LightTable,
    ambient_color: [f32; 4],
    scene_bind_group: Option<&'a wgpu::BindGroup>,
    scene_bind_group_layout: Option<&'a wgpu::BindGroupLayout>,
    directional_shadow_enabled: bool,
}

impl<'a> SceneLightingResources<'a> {
    #[inline]
    pub fn new(light_table: &'a LightTable, ambient_color: [f32; 4]) -> Self {
        Self {
            light_table,
            ambient_color,
            scene_bind_group: None,
            scene_bind_group_layout: None,
            directional_shadow_enabled: false,
        }
    }

    #[inline]
    pub fn with_scene_bind_group(
        mut self,
        bind_group: &'a wgpu::BindGroup,
        bind_group_layout: &'a wgpu::BindGroupLayout,
        directional_shadow_enabled: bool,
    ) -> Self {
        self.scene_bind_group = Some(bind_group);
        self.scene_bind_group_layout = Some(bind_group_layout);
        self.directional_shadow_enabled = directional_shadow_enabled;
        self
    }

    #[inline]
    pub fn light_table(&self) -> &'a LightTable {
        self.light_table
    }

    #[inline]
    pub fn light_bind_group(&self) -> &'a wgpu::BindGroup {
        self.light_table.bind_group()
    }

    #[inline]
    pub fn light_bind_group_layout(&self) -> &'a wgpu::BindGroupLayout {
        self.light_table.bind_group_layout()
    }

    #[inline]
    pub fn scene_bind_group(&self) -> Option<&'a wgpu::BindGroup> {
        self.scene_bind_group
    }

    #[inline]
    pub fn scene_bind_group_layout(&self) -> Option<&'a wgpu::BindGroupLayout> {
        self.scene_bind_group_layout
    }

    #[inline]
    pub fn ambient_color(&self) -> [f32; 4] {
        self.ambient_color
    }

    #[inline]
    pub fn light_count(&self) -> usize {
        self.light_table.len()
    }

    #[inline]
    pub fn point_light_count(&self) -> usize {
        self.light_table.point_count()
    }

    #[inline]
    pub fn directional_light_count(&self) -> usize {
        self.light_table.directional_count()
    }

    #[inline]
    pub fn spot_light_count(&self) -> usize {
        self.light_table.spot_count()
    }

    #[inline]
    pub fn directional_shadow_enabled(&self) -> bool {
        self.directional_shadow_enabled
    }
}

fn create_light_buffer(ctx: &GpuContext, capacity: usize) -> wgpu::Buffer {
    ctx.device().create_buffer(&wgpu::BufferDescriptor {
        label: Some("gpu_light_table"),
        size: (capacity.max(1) * std::mem::size_of::<GpuLight>()) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_device() -> (wgpu::Device, wgpu::Queue) {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .expect("No suitable GPU adapter found for light table tests");

        pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("light_table_test_device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
            },
            None,
        ))
        .expect("Failed to create test GPU device")
    }

    #[test]
    fn light_table_counts_gpu_light_kinds() {
        let (device, queue) = create_test_device();
        let ctx = GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [4, 4]);
        let mut table = LightTable::new(&ctx);
        table.set_all(
            &ctx,
            &[
                GpuLight {
                    pos_radius: [0.0, 1.0, 2.0, 3.0],
                    color: [1.0, 1.0, 1.0, 1.0],
                    falloff: [1.0, 0.0, 0.0, 0.0],
                    dir_shadow: [0.0, 0.0, 0.0, -1.0],
                },
                GpuLight {
                    pos_radius: [0.0, -1.0, 0.0, 0.0],
                    color: [0.5, 0.5, 0.5, 1.0],
                    falloff: [0.0, 1.0, 0.0, 0.0],
                    dir_shadow: [0.0, 0.0, 0.0, -1.0],
                },
                GpuLight {
                    pos_radius: [2.0, 3.0, 4.0, 8.0],
                    color: [0.25, 0.5, 1.0, 1.0],
                    falloff: [2.0, 2.0, 0.95, 0.8],
                    dir_shadow: [0.0, -1.0, 0.0, -1.0],
                },
            ],
        );

        assert_eq!(table.len(), 3);
        assert_eq!(table.point_count(), 1);
        assert_eq!(table.directional_count(), 1);
        assert_eq!(table.spot_count(), 1);
        assert!(table.lights()[0].is_point());
        assert!(table.lights()[1].is_directional());
        assert!(table.lights()[2].is_spot());
    }

    #[test]
    fn scene_lighting_resources_wrap_light_table_contract() {
        let (device, queue) = create_test_device();
        let ctx = GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [4, 4]);
        let table = LightTable::new(&ctx);
        let lighting = SceneLightingResources::new(&table, [0.1, 0.2, 0.3, 1.0]);

        assert_eq!(lighting.ambient_color(), [0.1, 0.2, 0.3, 1.0]);
        assert_eq!(lighting.light_count(), 0);
        assert!(std::ptr::eq(
            lighting.light_bind_group(),
            table.bind_group()
        ));
        assert!(std::ptr::eq(
            lighting.light_bind_group_layout(),
            table.bind_group_layout()
        ));
        assert!(lighting.scene_bind_group().is_none());
    }
}

fn create_light_bind_group(
    ctx: &GpuContext,
    layout: &wgpu::BindGroupLayout,
    buffer: &wgpu::Buffer,
    meta_buffer: &wgpu::Buffer,
) -> wgpu::BindGroup {
    ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("gpu_light_table_bg"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: meta_buffer.as_entire_binding(),
            },
        ],
    })
}

fn create_light_meta_buffer(ctx: &GpuContext) -> wgpu::Buffer {
    ctx.device().create_buffer(&wgpu::BufferDescriptor {
        label: Some("gpu_light_table_meta"),
        size: std::mem::size_of::<LightTableMeta>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}
