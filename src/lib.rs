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

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

pub mod ecs;
pub mod reflect;

#[cfg(feature = "asset")]
pub mod asset;

#[cfg(feature = "app")]
pub mod gpu;

#[cfg(feature = "app")]
pub mod render;

#[cfg(feature = "app")]
pub mod app;

#[cfg(feature = "audio")]
pub mod audio;
