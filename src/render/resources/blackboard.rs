//! Blackboard: named resource sharing across render graph passes.
//!
//! The blackboard is a typed key-value store that allows passes to publish
//! and consume resources by name without hard-wired coupling.  This is
//! directly inspired by SakuraEngine's RenderGraph blackboard system.
//!
//! # Example
//!
//! ```rust,ignore
//! // In a lighting pass setup:
//! blackboard.set("scene_color", hdr_texture);
//!
//! // In a post-processing pass:
//! let scene = blackboard.get::<TextureHandle>("scene_color").unwrap();
//! ```

use std::any::Any;
use std::borrow::Cow;

use rustc_hash::FxHashMap;

/// A typed key-value store for sharing resources between render graph passes.
///
/// Values are stored as `Box<dyn Any>` and retrieved with downcasting, so
/// the caller must know the concrete type at the retrieval site.
pub struct Blackboard {
    entries: FxHashMap<Cow<'static, str>, Box<dyn Any>>,
}

impl Default for Blackboard {
    fn default() -> Self {
        Self::new()
    }
}

impl Blackboard {
    /// Create an empty blackboard.
    pub fn new() -> Self {
        Self {
            entries: FxHashMap::default(),
        }
    }

    /// Store a value under the given name. Overwrites any existing entry.
    pub fn set<T: Any>(&mut self, name: impl Into<Cow<'static, str>>, value: T) {
        self.entries.insert(name.into(), Box::new(value));
    }

    /// Retrieve a shared reference to a value by name and type.
    ///
    /// Returns `None` if the key doesn't exist or the type doesn't match.
    pub fn get<T: Any>(&self, name: &str) -> Option<&T> {
        self.entries.get(name).and_then(|v| v.downcast_ref::<T>())
    }

    /// Retrieve a mutable reference to a value by name and type.
    pub fn get_mut<T: Any>(&mut self, name: &str) -> Option<&mut T> {
        self.entries
            .get_mut(name)
            .and_then(|v| v.downcast_mut::<T>())
    }

    /// Check whether a key exists.
    pub fn contains(&self, name: &str) -> bool {
        self.entries.contains_key(name)
    }

    /// Remove an entry, returning the value if the type matches.
    pub fn remove<T: Any>(&mut self, name: &str) -> Option<T> {
        self.entries
            .remove(name)
            .and_then(|v| v.downcast::<T>().ok())
            .map(|b| *b)
    }

    /// Remove all entries.
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_and_get() {
        let mut bb = Blackboard::new();
        bb.set("count", 42u32);
        assert_eq!(bb.get::<u32>("count"), Some(&42));
    }

    #[test]
    fn wrong_type_returns_none() {
        let mut bb = Blackboard::new();
        bb.set("count", 42u32);
        assert!(bb.get::<f32>("count").is_none());
    }

    #[test]
    fn missing_key_returns_none() {
        let bb = Blackboard::new();
        assert!(bb.get::<u32>("nope").is_none());
    }

    #[test]
    fn overwrite() {
        let mut bb = Blackboard::new();
        bb.set("val", 1u32);
        bb.set("val", 2u32);
        assert_eq!(bb.get::<u32>("val"), Some(&2));
    }

    #[test]
    fn remove() {
        let mut bb = Blackboard::new();
        bb.set("val", 10u32);
        let removed = bb.remove::<u32>("val");
        assert_eq!(removed, Some(10));
        assert!(!bb.contains("val"));
    }
}
