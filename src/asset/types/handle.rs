use std::any::{Any, TypeId};
use std::fmt::{Display, Formatter};
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Weak};

use serde::{Deserialize, Serialize};

use super::{AssetError, AssetId, AssetState, AssetStatus};
pub trait Asset: Send + Sync + 'static {
    const TYPE: &'static str;
}

pub(crate) trait AssetHandleProvider: Send + Sync {
    fn state_for_handle(&self, id: AssetId) -> AssetState;
    fn error_for_handle(&self, id: AssetId) -> Option<AssetError>;
    fn get_for_handle(
        &self,
        id: AssetId,
        expected_type: &'static str,
        expected_type_id: TypeId,
    ) -> Result<Arc<dyn Any + Send + Sync>, AssetError>;
}

pub(crate) struct AssetLease {
    id: AssetId,
    release_tx: Sender<AssetId>,
    provider: Option<Weak<dyn AssetHandleProvider>>,
}

impl AssetLease {
    pub(crate) fn new(
        id: AssetId,
        release_tx: Sender<AssetId>,
        provider: Weak<dyn AssetHandleProvider>,
    ) -> Self {
        Self {
            id,
            release_tx,
            provider: Some(provider),
        }
    }
}

impl Drop for AssetLease {
    fn drop(&mut self) {
        let _ = self.release_tx.send(self.id);
    }
}

/// Strong typed asset handle.
///
/// Cloning this handle keeps the same asset lease alive. Dropping the final
/// clone releases that lease back to the owning [`Assets`](crate::asset::Assets)
/// facade on the next asset update.
pub struct Handle<T: Asset> {
    id: AssetId,
    lease: Arc<AssetLease>,
    marker: PhantomData<fn() -> T>,
}

impl<T: Asset> Handle<T> {
    #[must_use]
    pub(crate) fn from_lease(id: AssetId, lease: Arc<AssetLease>) -> Self {
        Self {
            id,
            lease,
            marker: PhantomData,
        }
    }

    #[cfg(test)]
    #[allow(dead_code)]
    #[must_use]
    pub(crate) fn new(id: AssetId) -> Self {
        let (release_tx, _release_rx) = std::sync::mpsc::channel();
        Self {
            id,
            lease: Arc::new(AssetLease {
                id,
                release_tx,
                provider: None,
            }),
            marker: PhantomData,
        }
    }

    #[must_use]
    pub fn id(&self) -> AssetId {
        self.id
    }

    #[must_use]
    pub fn downgrade(&self) -> WeakHandle<T> {
        WeakHandle::new(self.id)
    }

    #[must_use]
    pub fn state(&self) -> AssetState {
        self.lease
            .provider
            .as_ref()
            .and_then(|provider| provider.upgrade())
            .map_or(AssetState::Unloaded, |provider| {
                provider.state_for_handle(self.id)
            })
    }

    #[must_use]
    pub fn status(&self) -> AssetStatus {
        self.state().into()
    }

    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.state() == AssetState::Installed
    }

    pub fn get(&self) -> Result<Arc<T>, AssetError> {
        let provider = self
            .lease
            .provider
            .as_ref()
            .and_then(|provider| provider.upgrade())
            .ok_or(AssetError::AssetNotInstalled {
                id: self.id,
                state: AssetState::Unloaded,
            })?;
        let installed = provider.get_for_handle(self.id, T::TYPE, TypeId::of::<T>())?;
        Arc::downcast::<T>(installed).map_err(|_| AssetError::AssetTypeMismatch {
            id: self.id,
            expected: T::TYPE,
            actual: "unknown".to_string(),
        })
    }

    #[must_use]
    pub fn try_get(&self) -> Option<Arc<T>> {
        self.get().ok()
    }

    #[must_use]
    pub fn error(&self) -> Option<AssetError> {
        self.lease
            .provider
            .as_ref()
            .and_then(|provider| provider.upgrade())
            .and_then(|provider| provider.error_for_handle(self.id))
    }
}

impl<T: Asset> Clone for Handle<T> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            lease: self.lease.clone(),
            marker: PhantomData,
        }
    }
}

impl<T: Asset> std::fmt::Debug for Handle<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Handle").field("id", &self.id).finish()
    }
}

impl<T: Asset> PartialEq for Handle<T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<T: Asset> Eq for Handle<T> {}

impl<T: Asset> std::hash::Hash for Handle<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

/// Typed asset source path.
///
/// This is an editor/serialized reference to an asset source key. It does not
/// keep runtime residency alive; resolve it through [`Assets`](crate::asset::Assets)
/// when a strong [`Handle<T>`] or weak [`WeakHandle<T>`] is needed.
pub struct AssetPath<T: Asset> {
    path: PathBuf,
    marker: PhantomData<fn() -> T>,
}

impl<T: Asset> AssetPath<T> {
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            marker: PhantomData,
        }
    }

    #[must_use]
    pub fn as_path(&self) -> &Path {
        &self.path
    }

    #[must_use]
    pub fn into_path_buf(self) -> PathBuf {
        self.path
    }
}

impl<T: Asset> Clone for AssetPath<T> {
    fn clone(&self) -> Self {
        Self::new(self.path.clone())
    }
}

impl<T: Asset> std::fmt::Debug for AssetPath<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("AssetPath").field(&self.path).finish()
    }
}

impl<T: Asset> Display for AssetPath<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.path.display())
    }
}

impl<T: Asset> PartialEq for AssetPath<T> {
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path
    }
}

impl<T: Asset> Eq for AssetPath<T> {}

impl<T: Asset> std::hash::Hash for AssetPath<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.path.hash(state);
    }
}

impl<T: Asset> AsRef<Path> for AssetPath<T> {
    fn as_ref(&self) -> &Path {
        self.as_path()
    }
}

impl<T: Asset> From<PathBuf> for AssetPath<T> {
    fn from(path: PathBuf) -> Self {
        Self::new(path)
    }
}

impl<T: Asset> From<&Path> for AssetPath<T> {
    fn from(path: &Path) -> Self {
        Self::new(path)
    }
}

impl<T: Asset> From<&str> for AssetPath<T> {
    fn from(path: &str) -> Self {
        Self::new(path)
    }
}

impl<T: Asset> Serialize for AssetPath<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.path.serialize(serializer)
    }
}

impl<'de, T: Asset> Deserialize<'de> for AssetPath<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        PathBuf::deserialize(deserializer).map(Self::new)
    }
}

/// Weak typed asset identity.
///
/// This does not keep the asset loaded. Use it for serialized data, editor
/// references, and places that need identity without residency.
#[derive(Debug)]
pub struct WeakHandle<T: Asset> {
    id: AssetId,
    marker: PhantomData<fn() -> T>,
}

impl<T: Asset> WeakHandle<T> {
    #[must_use]
    pub fn new(id: AssetId) -> Self {
        Self {
            id,
            marker: PhantomData,
        }
    }

    #[must_use]
    pub fn id(self) -> AssetId {
        self.id
    }
}

impl<T: Asset> Clone for WeakHandle<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: Asset> Copy for WeakHandle<T> {}

impl<T: Asset> PartialEq for WeakHandle<T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<T: Asset> Eq for WeakHandle<T> {}

impl<T: Asset> std::hash::Hash for WeakHandle<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}
