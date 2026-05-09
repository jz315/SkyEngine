pub mod ddgi;
pub mod ssgi;

pub(crate) fn register_default_providers(registry: &mut crate::render::gi::GiProviderRegistry) {
    registry.register(ddgi::DdgiProviderFactory);
    registry.register(ssgi::SsgiProviderFactory);
}
