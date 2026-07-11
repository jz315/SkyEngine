#[cfg_attr(not(test), allow(dead_code))]
pub fn create_model_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("draw_model_bgl"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: true,
                min_binding_size: Some(
                    std::num::NonZeroU64::new(std::mem::size_of::<[f32; 16]>() as u64)
                        .expect("ModelUniform has non-zero size"),
                ),
            },
            count: None,
        }],
    })
}

pub(super) const IDENTITY_MODEL: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0, //
    0.0, 1.0, 0.0, 0.0, //
    0.0, 0.0, 1.0, 0.0, //
    0.0, 0.0, 0.0, 1.0,
];
