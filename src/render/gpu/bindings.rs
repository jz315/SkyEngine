//! Bind group layout helpers for common renderer resources.

pub fn sampled_texture_entry(
    binding: u32,
    visibility: wgpu::ShaderStages,
    sample_type: wgpu::TextureSampleType,
    view_dimension: wgpu::TextureViewDimension,
    multisampled: bool,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Texture {
            sample_type,
            view_dimension,
            multisampled,
        },
        count: None,
    }
}

pub fn storage_texture_entry(
    binding: u32,
    visibility: wgpu::ShaderStages,
    access: wgpu::StorageTextureAccess,
    format: wgpu::TextureFormat,
    view_dimension: wgpu::TextureViewDimension,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::StorageTexture {
            access,
            format,
            view_dimension,
        },
        count: None,
    }
}

pub fn uniform_buffer_entry(
    binding: u32,
    visibility: wgpu::ShaderStages,
    has_dynamic_offset: bool,
    min_binding_size: Option<wgpu::BufferSize>,
) -> wgpu::BindGroupLayoutEntry {
    buffer_entry(
        binding,
        visibility,
        wgpu::BufferBindingType::Uniform,
        has_dynamic_offset,
        min_binding_size,
    )
}

pub fn storage_buffer_entry(
    binding: u32,
    visibility: wgpu::ShaderStages,
    read_only: bool,
    has_dynamic_offset: bool,
    min_binding_size: Option<wgpu::BufferSize>,
) -> wgpu::BindGroupLayoutEntry {
    buffer_entry(
        binding,
        visibility,
        wgpu::BufferBindingType::Storage { read_only },
        has_dynamic_offset,
        min_binding_size,
    )
}

pub fn sampler_entry(
    binding: u32,
    visibility: wgpu::ShaderStages,
    sampler_type: wgpu::SamplerBindingType,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Sampler(sampler_type),
        count: None,
    }
}

fn buffer_entry(
    binding: u32,
    visibility: wgpu::ShaderStages,
    ty: wgpu::BufferBindingType,
    has_dynamic_offset: bool,
    min_binding_size: Option<wgpu::BufferSize>,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Buffer {
            ty,
            has_dynamic_offset,
            min_binding_size,
        },
        count: None,
    }
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
        .expect("No suitable GPU adapter found for render tests");

        pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("render_bindings_test_device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
            },
            None,
        ))
        .expect("Failed to create test GPU device")
    }

    #[test]
    fn storage_texture_bind_group_accepts_d2_array_view() {
        let (device, _queue) = create_test_device();
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("storage_d2_array_texture"),
            size: wgpu::Extent3d {
                width: 8,
                height: 8,
                depth_or_array_layers: 4,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::STORAGE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("storage_d2_array_view"),
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            base_array_layer: 0,
            array_layer_count: Some(4),
            ..Default::default()
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("storage_d2_array_bgl"),
            entries: &[storage_texture_entry(
                0,
                wgpu::ShaderStages::COMPUTE,
                wgpu::StorageTextureAccess::WriteOnly,
                wgpu::TextureFormat::Rgba8Unorm,
                wgpu::TextureViewDimension::D2Array,
            )],
        });

        let _bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("storage_d2_array_bg"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            }],
        });
    }
}
