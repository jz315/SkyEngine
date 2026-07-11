//! Expert renderer API, grouped by responsibility.
//!
//! Application code should use [`crate::render`]. Renderer authors can opt
//! into the focused modules here instead of importing one flat, unstable API.

pub mod draw;
pub mod execution;
pub mod gpu;
pub mod graph;
pub mod resources;

#[cfg(feature = "live2d")]
pub mod live2d {
    pub use crate::render::features::live2d::{
        Live2DExpressionPlayer, Live2DLoadError, Live2DModel, Live2DModelResource, Live2DPhysics,
        Live2DPhysicsOptions, Live2DPose, Live2DRenderer, Live2DUserModel, PreparedLive2DFrame,
    };

    pub mod asset {
        pub use crate::render::features::live2d::asset::*;
    }

    pub mod model {
        pub use crate::render::features::live2d::model::*;
    }

    pub mod render {
        pub use crate::render::features::live2d::render::*;
    }

    pub mod runtime {
        pub use crate::render::features::live2d::runtime::*;
    }
}
