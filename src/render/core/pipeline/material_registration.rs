use std::any::TypeId;

pub(crate) struct MaterialRegistration {
    pub(crate) type_id: TypeId,
    pub(crate) type_name: &'static str,
    pub(crate) register:
        fn(&mut crate::render::resources::material::MaterialRegistry, &wgpu::Device),
}
