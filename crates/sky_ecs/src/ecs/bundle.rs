use super::{create_archetype, Archetype, Chunk, MAX_COMPONENTS};
use crate::ecs::{component_type, ComponentType};
use rustc_hash::FxHashMap;
use smallvec::{smallvec, SmallVec};
use std::{any::TypeId, cell::RefCell, ptr, sync::RwLock};

lazy_static::lazy_static! {
    static ref BUNDLE_META: RwLock<FxHashMap<TypeId, &'static BundleMeta>> = RwLock::new(FxHashMap::default());
}

thread_local! {
    static LOCAL_BUNDLE_META: RefCell<FxHashMap<TypeId, &'static BundleMeta>> = RefCell::new(FxHashMap::default());
}

pub(crate) struct BundleMeta {
    pub archetype: Archetype,
    pub columns: SmallVec<[(usize, usize); MAX_COMPONENTS]>,
}

mod sealed {
    pub trait BundleSealed {}
}

fn assert_unique_types(types: &[ComponentType]) {
    for (index, ty) in types.iter().enumerate() {
        for other in &types[(index + 1)..] {
            if ty.id() == other.id() {
                panic!(
                    "duplicate component type `{}` is not supported in bundles",
                    ty.name
                );
            }
        }
    }
}

fn bundle_meta<B: 'static>(
    make_types: impl FnOnce() -> SmallVec<[ComponentType; MAX_COMPONENTS]>,
) -> &'static BundleMeta {
    let type_id = TypeId::of::<B>();

    if let Some(meta) = LOCAL_BUNDLE_META.with(|cache| cache.borrow().get(&type_id).copied()) {
        return meta;
    }

    if let Some(meta) = BUNDLE_META.read().unwrap().get(&type_id).copied() {
        LOCAL_BUNDLE_META.with(|cache| {
            cache.borrow_mut().insert(type_id, meta);
        });
        return meta;
    }

    let mut cache = BUNDLE_META.write().unwrap();
    if let Some(meta) = cache.get(&type_id).copied() {
        LOCAL_BUNDLE_META.with(|cache| {
            cache.borrow_mut().insert(type_id, meta);
        });
        return meta;
    }

    let types = make_types();
    assert_unique_types(&types);
    let mut builder = create_archetype();
    for ty in &types {
        builder = builder.add_component(*ty);
    }
    let archetype = builder.build();
    let columns = types
        .iter()
        .map(|ty| {
            let component_index = archetype.query_component_index(ty).unwrap();
            (component_index, ty.size)
        })
        .collect();
    let meta = Box::leak(Box::new(BundleMeta { archetype, columns }));

    let meta = *cache.entry(type_id).or_insert(meta);
    LOCAL_BUNDLE_META.with(|cache| {
        cache.borrow_mut().insert(type_id, meta);
    });
    meta
}

/// Trait implemented by component tuples for spawning entities.
///
/// You do not need to implement this manually — it is auto-implemented
/// for tuples of up to 8 `'static` types.  Both `Copy` and non-`Copy`
/// types are supported.
///
/// This trait is sealed so Sky ECS can keep the storage invariants behind
/// [`World::spawn`](crate::ecs::World::spawn) sound.
pub trait Bundle: sealed::BundleSealed + 'static {
    fn cached_meta() -> (Archetype, &'static [(usize, usize)]);

    fn archetype() -> Archetype;
    /// # Safety
    ///
    /// `entity_index` must be a valid slot within `chunk` and the
    /// chunk's archetype must match the bundle's archetype.
    unsafe fn write(self, chunk: &mut Chunk, entity_index: usize);

    /// Fast write using pre-computed column indices. Skips binary search.
    ///
    /// # Safety
    ///
    /// `entity_index` must be a valid uninitialized slot within `chunk`.
    /// `columns` must come from this bundle's [`cached_meta`](Self::cached_meta)
    /// result and therefore match the chunk archetype and tuple element order.
    unsafe fn write_fast(self, chunk: &mut Chunk, entity_index: usize, columns: &[(usize, usize)]);
}

macro_rules! impl_bundle_tuple {
    ($(($Type:ident, $value:ident, $idx:tt)),+ $(,)?) => {
        impl<$($Type: 'static),+> sealed::BundleSealed for ($($Type,)+) {}

        impl<$($Type: 'static),+> Bundle for ($($Type,)+) {
            fn cached_meta() -> (Archetype, &'static [(usize, usize)]) {
                let meta = bundle_meta::<Self>(|| smallvec![$(component_type::<$Type>()),+]);
                (meta.archetype, &meta.columns)
            }

            fn archetype() -> Archetype {
                Self::cached_meta().0
            }

            unsafe fn write(self, chunk: &mut Chunk, entity_index: usize) {
                let (_, columns) = Self::cached_meta();
                self.write_fast(chunk, entity_index, columns);
            }

            unsafe fn write_fast(self, chunk: &mut Chunk, entity_index: usize, columns: &[(usize, usize)]) {
                let ($($value,)+) = self;

                $(
                    let (component_index, comp_size) = columns[$idx];
                    let col_ptr = chunk.column_ptr(component_index);
                    ptr::write(
                        col_ptr.add(comp_size * entity_index) as *mut $Type,
                        $value,
                    );
                )+
            }
        }
    };
}

impl_bundle_tuple!((A, a, 0));
impl_bundle_tuple!((A, a, 0), (B, b, 1));
impl_bundle_tuple!((A, a, 0), (B, b, 1), (C, c, 2));
impl_bundle_tuple!((A, a, 0), (B, b, 1), (C, c, 2), (D, d, 3));
impl_bundle_tuple!((A, a, 0), (B, b, 1), (C, c, 2), (D, d, 3), (E, e, 4));
impl_bundle_tuple!(
    (A, a, 0),
    (B, b, 1),
    (C, c, 2),
    (D, d, 3),
    (E, e, 4),
    (F, f, 5)
);
impl_bundle_tuple!(
    (A, a, 0),
    (B, b, 1),
    (C, c, 2),
    (D, d, 3),
    (E, e, 4),
    (F, f, 5),
    (G, g, 6)
);
impl_bundle_tuple!(
    (A, a, 0),
    (B, b, 1),
    (C, c, 2),
    (D, d, 3),
    (E, e, 4),
    (F, f, 5),
    (G, g, 6),
    (H, h, 7)
);
