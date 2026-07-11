//! Foundational runtime type identity and layout metadata.
//!
//! `sky_type` is the low-level type system shared by Sky ECS and higher-level
//! reflection. It records type identity, memory layout, and erased drop
//! callbacks without knowing about components, inspectors, math types, or
//! engine modules.

use rustc_hash::FxHashMap;
use std::alloc::Layout;
use std::any::{type_name, TypeId};
use std::cell::{Cell, RefCell};
use std::hash::{Hash, Hasher};
use std::ops::Deref;
use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

/// # Safety
///
/// The pointer must point to a valid, initialized value of the type this
/// function was created for. After the call the pointee is logically dropped
/// and must not be read again.
pub unsafe fn drop_in_place_erased<T>(ptr: *mut u8) {
    std::ptr::drop_in_place(ptr as *mut T);
}

/// Foundational runtime type handle.
///
/// This is a thin, copyable handle to leaked static layout metadata. Equality
/// and hashing use the metadata address, so handles are cheap to compare in hot
/// ECS paths.
#[derive(Debug, Clone, Copy)]
pub struct Type {
    info: &'static TypeInfo,
}

impl Type {
    fn new(info: &'static TypeInfo) -> Self {
        Self { info }
    }

    #[inline(always)]
    pub fn id(&self) -> usize {
        self.info as *const TypeInfo as usize
    }

    #[inline(always)]
    pub fn needs_drop(&self) -> bool {
        self.info.drop_fn.is_some()
    }

    #[inline(always)]
    pub fn drop_fn(&self) -> Option<unsafe fn(*mut u8)> {
        self.info.drop_fn
    }

    #[inline]
    pub fn rust_type_id(&self) -> Option<TypeId> {
        self.info.rust_type_id
    }
}

impl PartialEq for Type {
    fn eq(&self, other: &Self) -> bool {
        self.id() == other.id()
    }
}

impl Eq for Type {}

impl Hash for Type {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id().hash(state);
    }
}

impl Deref for Type {
    type Target = TypeInfo;

    fn deref(&self) -> &Self::Target {
        self.info
    }
}

/// Static layout metadata for one registered Rust or dynamic type.
#[derive(Debug)]
pub struct TypeInfo {
    pub size: usize,
    pub align: usize,
    pub name: String,
    pub drop_fn: Option<unsafe fn(*mut u8)>,
    rust_type_id: Option<TypeId>,
}

impl TypeInfo {
    fn new(
        name: &str,
        size: usize,
        align: usize,
        drop_fn: Option<unsafe fn(*mut u8)>,
        rust_type_id: Option<TypeId>,
    ) -> Self {
        Self {
            name: name.to_string(),
            size,
            align,
            drop_fn,
            rust_type_id,
        }
    }
}

lazy_static::lazy_static! {
    static ref TYPE_REGISTRY: RwLock<CoreTypeRegistry> = RwLock::new(CoreTypeRegistry::new());
}

thread_local! {
    static LAST_RUST_TYPE: Cell<Option<(TypeId, Type)>> = const { Cell::new(None) };
    static LOCAL_RUST_TYPES: RefCell<FxHashMap<TypeId, Type>> =
        RefCell::new(FxHashMap::default());
}

struct CoreTypeRegistry {
    name_to_type: FxHashMap<String, Type>,
    rust_type_to_type: FxHashMap<TypeId, Type>,
}

fn registry_read() -> RwLockReadGuard<'static, CoreTypeRegistry> {
    TYPE_REGISTRY
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn registry_write() -> RwLockWriteGuard<'static, CoreTypeRegistry> {
    TYPE_REGISTRY
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl CoreTypeRegistry {
    fn new() -> Self {
        Self {
            name_to_type: FxHashMap::default(),
            rust_type_to_type: FxHashMap::default(),
        }
    }

    fn validate_layout(name: &str, size: usize, align: usize) {
        assert!(!name.is_empty(), "type name must not be empty");
        assert!(
            align != 0 && align.is_power_of_two(),
            "type `{name}` has invalid alignment {align}; alignment must be a non-zero power of two"
        );
        assert!(
            Layout::from_size_align(size.max(1), align).is_ok(),
            "type `{name}` has invalid layout: size {size}, align {align}"
        );
    }

    fn register_layout(&mut self, name: &str, size: usize, align: usize) -> Type {
        Self::validate_layout(name, size, align);

        if let Some(ty) = self.name_to_type.get(name) {
            assert_eq!(
                ty.size, size,
                "type `{name}` was already registered with size {}, not {size}",
                ty.size
            );
            assert_eq!(
                ty.align, align,
                "type `{name}` was already registered with alignment {}, not {align}",
                ty.align
            );
            assert!(
                ty.rust_type_id.is_none() && ty.drop_fn.is_none(),
                "type `{name}` is already registered as a Rust type and cannot be re-registered as an opaque dynamic type"
            );
            return *ty;
        }

        let info = Box::leak(Box::new(TypeInfo::new(name, size, align, None, None)));
        let ty = Type::new(info);
        self.name_to_type.insert(name.to_string(), ty);
        ty
    }

    fn register_rust<T: 'static>(&mut self) -> Type {
        let rust_type_id = TypeId::of::<T>();
        if let Some(ty) = self.rust_type_to_type.get(&rust_type_id) {
            return *ty;
        }

        let name = type_name::<T>();
        if let Some(ty) = self.name_to_type.get(name).copied() {
            if ty.rust_type_id == Some(rust_type_id) {
                self.rust_type_to_type.insert(rust_type_id, ty);
                return ty;
            }

            panic!(
                "type name `{name}` is already registered for a different or opaque layout; Rust type registration would lose layout/drop guarantees"
            );
        }

        let drop_fn = if std::mem::needs_drop::<T>() {
            Some(drop_in_place_erased::<T> as unsafe fn(*mut u8))
        } else {
            None
        };
        let info = Box::leak(Box::new(TypeInfo::new(
            name,
            core::mem::size_of::<T>(),
            core::mem::align_of::<T>(),
            drop_fn,
            Some(rust_type_id),
        )));
        let ty = Type::new(info);
        self.name_to_type.insert(name.to_string(), ty);
        self.rust_type_to_type.insert(rust_type_id, ty);
        ty
    }

    fn query_by_name(&self, name: &str) -> Option<Type> {
        self.name_to_type.get(name).copied()
    }

    fn query_by_rust_type<T: 'static>(&self) -> Option<Type> {
        self.rust_type_to_type.get(&TypeId::of::<T>()).copied()
    }
}

/// Registers a dynamic opaque type by name and layout.
pub fn register(name: &str, size: usize, align: usize) -> Type {
    let mut registry = registry_write();
    registry.register_layout(name, size, align)
}

/// Registers or fetches the foundational type handle for `T`.
pub fn type_of<T: 'static>() -> Type {
    let rust_type_id = TypeId::of::<T>();

    if let Some((cached_type_id, ty)) = LAST_RUST_TYPE.get() {
        if cached_type_id == rust_type_id {
            return ty;
        }
    }

    if let Some(ty) = LOCAL_RUST_TYPES.with(|cache| cache.borrow().get(&rust_type_id).copied()) {
        LAST_RUST_TYPE.set(Some((rust_type_id, ty)));
        return ty;
    }

    {
        let registry = registry_read();
        if let Some(ty) = registry.query_by_rust_type::<T>() {
            LOCAL_RUST_TYPES.with(|cache| {
                cache.borrow_mut().insert(rust_type_id, ty);
            });
            LAST_RUST_TYPE.set(Some((rust_type_id, ty)));
            return ty;
        }
    }

    let mut registry = registry_write();
    let ty = registry.register_rust::<T>();
    LOCAL_RUST_TYPES.with(|cache| {
        cache.borrow_mut().insert(rust_type_id, ty);
    });
    LAST_RUST_TYPE.set(Some((rust_type_id, ty)));
    ty
}

/// Queries a foundational type by registered name.
pub fn query_by_name(name: &str) -> Option<Type> {
    let registry = registry_read();
    registry.query_by_name(name)
}

/// Queries a Rust type if it was already registered.
pub fn query_by_rust_type<T: 'static>() -> Option<Type> {
    let rust_type_id = TypeId::of::<T>();
    if let Some((cached_type_id, ty)) = LAST_RUST_TYPE.get() {
        if cached_type_id == rust_type_id {
            return Some(ty);
        }
    }

    if let Some(ty) = LOCAL_RUST_TYPES.with(|cache| cache.borrow().get(&rust_type_id).copied()) {
        LAST_RUST_TYPE.set(Some((rust_type_id, ty)));
        return Some(ty);
    }

    let registry = registry_read();
    let ty = registry.query_by_rust_type::<T>()?;
    LOCAL_RUST_TYPES.with(|cache| {
        cache.borrow_mut().insert(rust_type_id, ty);
    });
    LAST_RUST_TYPE.set(Some((rust_type_id, ty)));
    Some(ty)
}

/// Returns all currently registered foundational types.
pub fn registered_types() -> Vec<Type> {
    let registry = registry_read();
    registry.name_to_type.values().copied().collect()
}

#[cfg(test)]
mod tests {
    use super::{register, type_of};

    #[test]
    fn repeated_dynamic_registration_requires_identical_layout() {
        let first = register("sky_type::tests::repeat_dynamic_layout", 16, 8);
        let second = register("sky_type::tests::repeat_dynamic_layout", 16, 8);

        assert_eq!(first.id(), second.id());
    }

    #[test]
    #[should_panic(expected = "already registered with size")]
    fn dynamic_registration_rejects_name_reuse_with_different_size() {
        register("sky_type::tests::different_dynamic_size", 4, 4);
        register("sky_type::tests::different_dynamic_size", 8, 4);
    }

    #[test]
    #[should_panic(expected = "invalid alignment")]
    fn dynamic_registration_rejects_non_power_of_two_alignment() {
        register("sky_type::tests::invalid_alignment", 4, 3);
    }

    #[test]
    #[should_panic(expected = "opaque layout")]
    fn rust_registration_rejects_prior_dynamic_name_collision() {
        #[allow(dead_code)]
        struct Collision(std::rc::Rc<()>);

        register(
            std::any::type_name::<Collision>(),
            std::mem::size_of::<Collision>(),
            std::mem::align_of::<Collision>(),
        );
        let _ = type_of::<Collision>();
    }

    #[test]
    #[should_panic(expected = "already registered as a Rust type")]
    fn dynamic_registration_rejects_existing_rust_type_name() {
        #[allow(dead_code)]
        struct RegisteredRust(u32);

        let _ = type_of::<RegisteredRust>();
        register(
            std::any::type_name::<RegisteredRust>(),
            std::mem::size_of::<RegisteredRust>(),
            std::mem::align_of::<RegisteredRust>(),
        );
    }
}
