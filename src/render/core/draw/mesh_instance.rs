use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct MeshPhaseInstance {
    model_col0: [f32; 4],
    model_col1: [f32; 4],
    model_col2: [f32; 4],
    model_col3: [f32; 4],
}

impl MeshPhaseInstance {
    #[inline]
    fn from_model(model: [f32; 16]) -> Self {
        Self {
            model_col0: [model[0], model[1], model[2], model[3]],
            model_col1: [model[4], model[5], model[6], model[7]],
            model_col2: [model[8], model[9], model[10], model[11]],
            model_col3: [model[12], model[13], model[14], model[15]],
        }
    }
}

pub(super) fn mesh_phase_instance_buffer(
    device: &wgpu::Device,
    label: &'static str,
    models: &[[f32; 16]],
) -> wgpu::Buffer {
    let instances: Vec<MeshPhaseInstance> = models
        .iter()
        .copied()
        .map(MeshPhaseInstance::from_model)
        .collect();
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: bytemuck::cast_slice(&instances),
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
    })
}

pub(super) fn mesh_phase_instance_layout<'a>() -> wgpu::VertexBufferLayout<'a> {
    const ATTRS: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![
        8 => Float32x4,
        9 => Float32x4,
        10 => Float32x4,
        11 => Float32x4
    ];
    wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<MeshPhaseInstance>() as u64,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &ATTRS,
    }
}
