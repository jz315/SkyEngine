//! Input system — raw state tracking and action-based abstraction.
//!
//! This module provides two layers of input handling:
//!
//! - **Raw layer** ([`Input`], [`KeyCode`], [`MouseButton`]) — direct
//!   per-frame keyboard and mouse state queries.  Zero-allocation,
//!   bit-array backed.
//!
//! - **Action layer** ([`InputActions`], [`ActionMap`], [`InputSource`]) —
//!   semantic game actions (e.g. "jump", "move") mapped to physical inputs.
//!   Supports composite axes, diagonal normalisation, runtime rebinding,
//!   and context switching via enable/disable.
//!
//! The raw layer is always available.  The action layer is opt-in — create
//! an [`InputActions`] resource and register [`ActionMap`]s during setup.
//!
//! # Quick start
//!
//! ```rust,ignore
//! use sky_engine::{ActionMap, InputActions, InputSource, KeyCode};
//!
//! // In setup:
//! let mut map = ActionMap::new("player");
//! map.add_button("jump", [InputSource::Key(KeyCode::Space)]);
//! map.add_axis_2d("move",
//!     InputSource::Key(KeyCode::KeyW),
//!     InputSource::Key(KeyCode::KeyS),
//!     InputSource::Key(KeyCode::KeyA),
//!     InputSource::Key(KeyCode::KeyD),
//! );
//!
//! let mut actions = InputActions::new();
//! actions.add_map(map);
//! world.insert_resource(actions);
//!
//! // In update:
//! let actions = world.get_resource::<InputActions>().unwrap();
//! if actions.action_pressed("jump") { /* ... */ }
//! let [dx, dy] = actions.axis_value("move");
//! ```

pub mod action;
pub mod actions;
pub mod interaction;
pub mod raw;
pub mod source;

// ── Re-exports: raw layer ───────────────────────────────────────────────────
pub use raw::{Input, KeyCode, MouseButton};

// ── Re-exports: action layer ────────────────────────────────────────────────
pub use action::{ActionKind, ActionMap, ActionValue};
pub use actions::InputActions;
pub use interaction::{InteractionCapture, InteractionContext, InteractionOwner};
pub use source::{InputSource, MouseAxisKind};
