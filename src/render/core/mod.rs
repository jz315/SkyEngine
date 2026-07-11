//! Renderer internals with no dependency on concrete renderer families.
//!
//! `core` provides the reusable frame, graph, GPU, draw, view, extraction,
//! pipeline, and resource contracts. Feature implementations may depend on it;
//! it must not depend on a concrete feature or backend.

pub(crate) mod draw;
pub(crate) mod execution;
pub(crate) mod extraction;
pub(crate) mod gpu;
pub(crate) mod graph;
pub(crate) mod pipeline;
pub(crate) mod resources;
pub(crate) mod runtime;
pub(crate) mod scene;
pub(crate) mod view;
