use super::{create_archetype, Archetype, Chunk};
use crate::reflect::{register_rust_type, Type};
use std::{any::TypeId, collections::HashMap, ptr, sync::RwLock};

lazy_static::lazy_static! {
    static ref BUNDLE_ARCHETYPES: RwLock<HashMap<TypeId, Archetype>> = RwLock::new(HashMap::new());
}

fn assert_unique_types(types: &[Type]) {
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

fn bundle_archetype<B: 'static>(types: &[Type]) -> Archetype {
    let type_id = TypeId::of::<B>();

    if let Some(archetype) = BUNDLE_ARCHETYPES.read().unwrap().get(&type_id).copied() {
        return archetype;
    }

    assert_unique_types(types);

    let mut builder = create_archetype();
    for ty in types {
        builder = builder.add_component(*ty);
    }
    let archetype = builder.build();

    let mut cache = BUNDLE_ARCHETYPES.write().unwrap();
    *cache.entry(type_id).or_insert(archetype)
}

pub trait Bundle: 'static {
    fn archetype() -> Archetype;
    /// # Safety
    ///
    /// `entity_index` must be a valid slot within `chunk` and the
    /// chunk's archetype must match the bundle's archetype.
    unsafe fn write(self, chunk: &mut Chunk, entity_index: usize);
}

macro_rules! impl_bundle_tuple {
    ($(($Type:ident, $value:ident)),+ $(,)?) => {
        impl<$($Type: Copy + 'static),+> Bundle for ($($Type,)+) {
            fn archetype() -> Archetype {
                bundle_archetype::<Self>(&[$(register_rust_type::<$Type>()),+])
            }

            unsafe fn write(self, chunk: &mut Chunk, entity_index: usize) {
                let ($($value,)+) = self;

                $(
                    let component_index = chunk
                        .archetype
                        .query_component_index(&register_rust_type::<$Type>())
                        .unwrap();
                    ptr::write(
                        chunk.component_ptr(component_index, entity_index) as *mut $Type,
                        $value,
                    );
                )+
            }
        }
    };
}

impl_bundle_tuple!((A, a));
impl_bundle_tuple!((A, a), (B, b));
impl_bundle_tuple!((A, a), (B, b), (C, c));
impl_bundle_tuple!((A, a), (B, b), (C, c), (D, d));
impl_bundle_tuple!((A, a), (B, b), (C, c), (D, d), (E, e));
impl_bundle_tuple!((A, a), (B, b), (C, c), (D, d), (E, e), (F, f));
impl_bundle_tuple!((A, a), (B, b), (C, c), (D, d), (E, e), (F, f), (G, g));
impl_bundle_tuple!(
    (A, a),
    (B, b),
    (C, c),
    (D, d),
    (E, e),
    (F, f),
    (G, g),
    (H, h)
);
