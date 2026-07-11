//! Mesh-owned normal and material prepasses.

mod material;
mod normal;
#[cfg(test)]
mod tests;

pub use material::SceneMaterialPrepass;
pub use normal::SceneNormalPrepass;
