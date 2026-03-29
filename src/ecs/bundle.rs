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

    /// Returns pre-computed column offsets for this bundle's component types
    /// within the given archetype. Used to avoid repeated lookups in batch ops.
    fn column_offsets(archetype: Archetype) -> Vec<(usize, usize)>;

    /// # Safety
    ///
    /// Fast write using pre-computed column offsets. Skips binary search.
    unsafe fn write_fast(self, chunk: &mut Chunk, entity_index: usize, offsets: &[(usize, usize)]);
}

macro_rules! impl_bundle_tuple {
    ($(($Type:ident, $value:ident, $idx:tt)),+ $(,)?) => {
        impl<$($Type: Copy + 'static),+> Bundle for ($($Type,)+) {
            fn archetype() -> Archetype {
                bundle_archetype::<Self>(&[$(register_rust_type::<$Type>()),+])
            }

            unsafe fn write(self, chunk: &mut Chunk, entity_index: usize) {
                let ($($value,)+) = self;

                $(
                    let ty = register_rust_type::<$Type>();
                    let component_index = chunk
                        .archetype
                        .query_component_index(&ty)
                        .unwrap();
                    let col_ptr = chunk.column_ptr(component_index);
                    ptr::write(
                        col_ptr.add(ty.size * entity_index) as *mut $Type,
                        $value,
                    );
                )+
            }

            fn column_offsets(archetype: Archetype) -> Vec<(usize, usize)> {
                vec![
                    $({
                        let ty = register_rust_type::<$Type>();
                        let ci = archetype.query_component_index(&ty).unwrap();
                        (archetype.layout[ci], ty.size)
                    },)+
                ]
            }

            unsafe fn write_fast(self, chunk: &mut Chunk, entity_index: usize, offsets: &[(usize, usize)]) {
                let base = chunk.data_ptr();
                let ($($value,)+) = self;

                $(
                    let (col_offset, comp_size) = offsets[$idx];
                    ptr::write(
                        base.add(col_offset + comp_size * entity_index) as *mut $Type,
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
impl_bundle_tuple!((A, a, 0), (B, b, 1), (C, c, 2), (D, d, 3), (E, e, 4), (F, f, 5));
impl_bundle_tuple!((A, a, 0), (B, b, 1), (C, c, 2), (D, d, 3), (E, e, 4), (F, f, 5), (G, g, 6));
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
