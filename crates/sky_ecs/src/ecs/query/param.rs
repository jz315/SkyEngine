use super::resolve_column_ptr;
use super::{Chunk, QueryComponent, QueryDescriptor};
use crate::ecs::component_type;
use core::slice;
use smallvec::SmallVec;

mod sealed {
    pub trait QuerySpecSealed {}
}

pub trait QueryParam {
    type Slice<'w>;
    type Item<'w>;

    fn component() -> QueryComponent;
    unsafe fn slice_from_raw<'w>(ptr: *mut u8, len: usize) -> Self::Slice<'w>;
    unsafe fn item_from_raw<'w>(ptr: *mut u8, index: usize) -> Self::Item<'w>;
}

pub trait ReadOnlyQueryParam: QueryParam {}

impl<T: 'static> QueryParam for &T {
    type Slice<'w> = &'w [T];
    type Item<'w> = &'w T;

    #[inline(always)]
    fn component() -> QueryComponent {
        QueryComponent::new(component_type::<T>(), false)
    }

    #[inline(always)]
    unsafe fn slice_from_raw<'w>(ptr: *mut u8, len: usize) -> Self::Slice<'w> {
        slice::from_raw_parts(ptr as *const T, len)
    }

    #[inline(always)]
    unsafe fn item_from_raw<'w>(ptr: *mut u8, index: usize) -> Self::Item<'w> {
        &*((ptr as *const T).add(index))
    }
}

impl<T: 'static> ReadOnlyQueryParam for &T {}

impl<T: 'static> QueryParam for &mut T {
    type Slice<'w> = &'w mut [T];
    type Item<'w> = &'w mut T;

    #[inline(always)]
    fn component() -> QueryComponent {
        QueryComponent::new(component_type::<T>(), true)
    }

    #[inline(always)]
    unsafe fn slice_from_raw<'w>(ptr: *mut u8, len: usize) -> Self::Slice<'w> {
        slice::from_raw_parts_mut(ptr as *mut T, len)
    }

    #[inline(always)]
    unsafe fn item_from_raw<'w>(ptr: *mut u8, index: usize) -> Self::Item<'w> {
        &mut *((ptr as *mut T).add(index))
    }
}

impl<T: 'static> QueryParam for Option<&T> {
    type Slice<'w> = Option<&'w [T]>;
    type Item<'w> = Option<&'w T>;

    #[inline(always)]
    fn component() -> QueryComponent {
        QueryComponent::optional(component_type::<T>(), false)
    }

    #[inline(always)]
    unsafe fn slice_from_raw<'w>(ptr: *mut u8, len: usize) -> Self::Slice<'w> {
        if ptr.is_null() {
            None
        } else {
            Some(slice::from_raw_parts(ptr as *const T, len))
        }
    }

    #[inline(always)]
    unsafe fn item_from_raw<'w>(ptr: *mut u8, index: usize) -> Self::Item<'w> {
        if ptr.is_null() {
            None
        } else {
            Some(&*((ptr as *const T).add(index)))
        }
    }
}

impl<T: 'static> ReadOnlyQueryParam for Option<&T> {}

impl<T: 'static> QueryParam for Option<&mut T> {
    type Slice<'w> = Option<&'w mut [T]>;
    type Item<'w> = Option<&'w mut T>;

    #[inline(always)]
    fn component() -> QueryComponent {
        QueryComponent::optional(component_type::<T>(), true)
    }

    #[inline(always)]
    unsafe fn slice_from_raw<'w>(ptr: *mut u8, len: usize) -> Self::Slice<'w> {
        if ptr.is_null() {
            None
        } else {
            Some(slice::from_raw_parts_mut(ptr as *mut T, len))
        }
    }

    #[inline(always)]
    unsafe fn item_from_raw<'w>(ptr: *mut u8, index: usize) -> Self::Item<'w> {
        if ptr.is_null() {
            None
        } else {
            Some(&mut *((ptr as *mut T).add(index)))
        }
    }
}

// ---------------------------------------------------------------------------
// QuerySpec
// ---------------------------------------------------------------------------

/// Type-level description of a typed query.
///
/// This trait is sealed; use the built-in query parameter forms (`&T`,
/// `&mut T`, `Option<&T>`, `Option<&mut T>`, and tuples of those) rather
/// than implementing it directly.
pub trait QuerySpec: sealed::QuerySpecSealed {
    type Chunk<'w>;
    type Item<'w>;

    fn descriptor() -> QueryDescriptor;
    unsafe fn chunk_from_raw<'w>(chunk: &'w Chunk, component_indices: &[u8]) -> Self::Chunk<'w>;
    unsafe fn for_each_entity<'w, Func>(chunk: &'w Chunk, component_indices: &[u8], f: &mut Func)
    where
        Func: FnMut(Self::Item<'w>);
}

pub trait ReadOnlyQuerySpec: QuerySpec {}

impl<P: QueryParam> sealed::QuerySpecSealed for P {}

impl<P: QueryParam> QuerySpec for P {
    type Chunk<'w> = P::Slice<'w>;
    type Item<'w> = P::Item<'w>;

    #[inline(always)]
    fn descriptor() -> QueryDescriptor {
        let mut components = SmallVec::new();
        components.push(P::component());
        QueryDescriptor::new(components)
    }

    #[inline(always)]
    unsafe fn chunk_from_raw<'w>(chunk: &'w Chunk, component_indices: &[u8]) -> Self::Chunk<'w> {
        P::slice_from_raw(
            resolve_column_ptr(chunk, component_indices[0]),
            chunk.entity_count,
        )
    }

    #[inline(always)]
    unsafe fn for_each_entity<'w, Func>(chunk: &'w Chunk, component_indices: &[u8], f: &mut Func)
    where
        Func: FnMut(Self::Item<'w>),
    {
        let base = resolve_column_ptr(chunk, component_indices[0]);
        for entity_index in 0..chunk.entity_count {
            f(P::item_from_raw(base, entity_index));
        }
    }
}

impl<P: ReadOnlyQueryParam> ReadOnlyQuerySpec for P {}

macro_rules! impl_query_spec_tuple {
    ($(($Param:ident, $base:ident, $index:tt)),+ $(,)?) => {
        impl<$($Param: QueryParam),+> sealed::QuerySpecSealed for ($($Param,)+) {}

        impl<$($Param: QueryParam),+> QuerySpec for ($($Param,)+) {
            type Chunk<'w> = ($($Param::Slice<'w>,)+);
            type Item<'w> = ($($Param::Item<'w>,)+);

            #[inline(always)]
            fn descriptor() -> QueryDescriptor {
                let mut components = SmallVec::new();
                $(components.push($Param::component());)+
                QueryDescriptor::new(components)
            }

            #[inline(always)]
            unsafe fn chunk_from_raw<'w>(
                chunk: &'w Chunk,
                component_indices: &[u8],
            ) -> Self::Chunk<'w> {
                (
                    $(
                        $Param::slice_from_raw(
                            resolve_column_ptr(chunk, component_indices[$index]),
                            chunk.entity_count,
                        ),
                    )+
                )
            }

            #[inline(always)]
            unsafe fn for_each_entity<'w, Func>(
                chunk: &'w Chunk,
                component_indices: &[u8],
                f: &mut Func,
            )
            where
                Func: FnMut(Self::Item<'w>),
            {
                $(let $base = resolve_column_ptr(chunk, component_indices[$index]);)+

                for entity_index in 0..chunk.entity_count {
                    f((
                        $(
                            $Param::item_from_raw($base, entity_index),
                        )+
                    ));
                }
            }
        }

        impl<$($Param: ReadOnlyQueryParam),+> ReadOnlyQuerySpec for ($($Param,)+) {}
    };
}

impl_query_spec_tuple!((A, a, 0), (B, b, 1));
impl_query_spec_tuple!((A, a, 0), (B, b, 1), (C, c, 2));
impl_query_spec_tuple!((A, a, 0), (B, b, 1), (C, c, 2), (D, d, 3));
impl_query_spec_tuple!((A, a, 0), (B, b, 1), (C, c, 2), (D, d, 3), (E, e, 4));
impl_query_spec_tuple!(
    (A, a, 0),
    (B, b, 1),
    (C, c, 2),
    (D, d, 3),
    (E, e, 4),
    (F, f, 5)
);
impl_query_spec_tuple!(
    (A, a, 0),
    (B, b, 1),
    (C, c, 2),
    (D, d, 3),
    (E, e, 4),
    (F, f, 5),
    (G, g, 6)
);
impl_query_spec_tuple!(
    (A, a, 0),
    (B, b, 1),
    (C, c, 2),
    (D, d, 3),
    (E, e, 4),
    (F, f, 5),
    (G, g, 6),
    (H, h, 7)
);
