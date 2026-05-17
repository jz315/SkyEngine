//! # Sky Engine
//!
//! A high-performance, archetype-based Entity Component System (ECS) library
//! for Rust.
//!
//! Sky Engine organises entities into **archetypes** (unique sets of component
//! types) and stores component data in fixed-size **chunks** using columnar
//! layout.  This design yields excellent cache locality during iteration and
//! supports both typed and dynamic queries.
//!
//! Engine-owned math types are available from `sky_engine::math`, currently
//! backed by `glam` internally.
//!
//! ## Quick Start
//!
//! ```rust
//! use sky_engine::ecs::{World, Commands};
//!
//! #[derive(Clone, Copy)]
//! struct Position { x: f32, y: f32 }
//!
//! #[derive(Clone, Copy)]
//! struct Velocity { x: f32, y: f32 }
//!
//! let mut world = World::new();
//! world.spawn((Position { x: 0.0, y: 0.0 }, Velocity { x: 1.0, y: 2.0 }));
//!
//! let mut query = world.query::<(&mut Position, &Velocity)>();
//! query.for_each(&world, |(pos, vel)| {
//!     pos.x += vel.x;
//!     pos.y += vel.y;
//! });
//! ```
//!
//! ## Component Requirements
//!
//! Components must be `'static`.  Both `Copy` and non-`Copy` types (e.g.
//! `String`, `Vec<T>`) are supported — destructors are called automatically
//! when entities are despawned or the world is dropped.

use mimalloc::MiMalloc;

extern crate self as sky_engine;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

pub mod action_queue;
pub mod diagnostics;
pub mod ecs;
pub mod math;
pub mod plugin;
pub mod reflect;

#[cfg(feature = "platform")]
pub mod platform;

#[cfg(feature = "scene")]
pub mod scene;

#[cfg(feature = "physics")]
pub mod physics;

#[cfg(feature = "asset")]
pub mod asset;

#[cfg(feature = "app")]
pub mod gpu;

#[cfg(feature = "app")]
pub mod render;

#[cfg(feature = "app")]
pub mod tile;

#[cfg(feature = "app")]
pub mod input;

#[cfg(feature = "app")]
pub mod app;

#[cfg(any(feature = "ui-core", feature = "ui-legacy", feature = "yakui-ui"))]
pub mod ui;

#[cfg(feature = "audio")]
pub mod audio;

#[cfg(feature = "video")]
pub mod video;

#[cfg(feature = "vn")]
pub mod vn;
