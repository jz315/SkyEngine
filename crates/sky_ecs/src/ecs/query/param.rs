use super::resolve_column_ptr;
use super::{Chunk, QueryComponent, QueryDescriptor};
use crate::ecs::component_type;
use core::slice;
use smallvec::SmallVec;

/// One component access inside a typed query specification.
///
/// # Safety
///
/// Implementations must return the exact component and access mode represented
/// by the type, and may only construct references of that component type from
/// initialized, correctly aligned ranges supplied by the query executor.
pub unsafe trait QueryParam {
    type Slice<'w>;
    type Item<'w>;

    fn component() -> QueryComponent;

    /// # Safety
    ///
    /// `ptr` must address a live, correctly aligned column of this component
    /// type, and `start..start + len` must be initialized and in bounds. The
    /// caller must uphold the access mode represented by this parameter.
    unsafe fn slice_from_raw<'w>(ptr: *mut u8, start: usize, len: usize) -> Self::Slice<'w>;

    /// # Safety
    ///
    /// `ptr` must address a live, correctly aligned column of this component
    /// type and `index` must select an initialized in-bounds row. The caller
    /// must uphold the access mode represented by this parameter.
    unsafe fn item_from_raw<'w>(ptr: *mut u8, index: usize) -> Self::Item<'w>;
}

/// Marker for query parameters that never construct mutable references.
///
/// # Safety
///
/// The underlying [`QueryParam`] implementation must expose shared access only.
pub unsafe trait ReadOnlyQueryParam: QueryParam {}

unsafe impl<T: 'static> QueryParam for &T {
    type Slice<'w> = &'w [T];
    type Item<'w> = &'w T;

    #[inline(always)]
    fn component() -> QueryComponent {
        QueryComponent::new(component_type::<T>(), false)
    }

    #[inline(always)]
    unsafe fn slice_from_raw<'w>(ptr: *mut u8, start: usize, len: usize) -> Self::Slice<'w> {
        slice::from_raw_parts((ptr as *const T).add(start), len)
    }

    #[inline(always)]
    unsafe fn item_from_raw<'w>(ptr: *mut u8, index: usize) -> Self::Item<'w> {
        &*((ptr as *const T).add(index))
    }
}

unsafe impl<T: 'static> ReadOnlyQueryParam for &T {}

unsafe impl<T: 'static> QueryParam for &mut T {
    type Slice<'w> = &'w mut [T];
    type Item<'w> = &'w mut T;

    #[inline(always)]
    fn component() -> QueryComponent {
        QueryComponent::new(component_type::<T>(), true)
    }

    #[inline(always)]
    unsafe fn slice_from_raw<'w>(ptr: *mut u8, start: usize, len: usize) -> Self::Slice<'w> {
        slice::from_raw_parts_mut((ptr as *mut T).add(start), len)
    }

    #[inline(always)]
    unsafe fn item_from_raw<'w>(ptr: *mut u8, index: usize) -> Self::Item<'w> {
        &mut *((ptr as *mut T).add(index))
    }
}

unsafe impl<T: 'static> QueryParam for Option<&T> {
    type Slice<'w> = Option<&'w [T]>;
    type Item<'w> = Option<&'w T>;

    #[inline(always)]
    fn component() -> QueryComponent {
        QueryComponent::optional(component_type::<T>(), false)
    }

    #[inline(always)]
    unsafe fn slice_from_raw<'w>(ptr: *mut u8, start: usize, len: usize) -> Self::Slice<'w> {
        if ptr.is_null() {
            None
        } else {
            Some(slice::from_raw_parts((ptr as *const T).add(start), len))
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

unsafe impl<T: 'static> ReadOnlyQueryParam for Option<&T> {}

unsafe impl<T: 'static> QueryParam for Option<&mut T> {
    type Slice<'w> = Option<&'w mut [T]>;
    type Item<'w> = Option<&'w mut T>;

    #[inline(always)]
    fn component() -> QueryComponent {
        QueryComponent::optional(component_type::<T>(), true)
    }

    #[inline(always)]
    unsafe fn slice_from_raw<'w>(ptr: *mut u8, start: usize, len: usize) -> Self::Slice<'w> {
        if ptr.is_null() {
            None
        } else {
            Some(slice::from_raw_parts_mut((ptr as *mut T).add(start), len))
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
/// This is an unsafe implementation contract. Use the built-in query
/// parameter forms or `#[derive(QueryData)]` rather than implementing it
/// manually.
///
/// # Safety
///
/// Implementations must describe every component access accurately, preserve
/// the aliasing mode of each parameter, and only construct references within
/// the storage and lifetime represented by the supplied chunk.
pub unsafe trait QuerySpec {
    type Chunk<'w>;
    type Item<'w>;

    fn descriptor() -> QueryDescriptor;

    /// Builds the typed slice view for one matching chunk.
    ///
    /// # Safety
    ///
    /// `component_indices` must come from this specification's descriptor for
    /// `chunk`, and the caller must uphold every declared shared/exclusive
    /// component access for `'w`.
    unsafe fn chunk_from_raw<'w>(chunk: &'w Chunk, component_indices: &[u8]) -> Self::Chunk<'w>;

    /// Builds a typed slice view over a prevalidated subrange of raw columns.
    ///
    /// # Safety
    ///
    /// Every pointer must address the corresponding descriptor column, the
    /// range must be initialized and in bounds, and accesses must be disjoint
    /// wherever the specification declares mutable references.
    #[doc(hidden)]
    unsafe fn chunk_from_raw_parts<'w>(
        component_ptrs: &[*mut u8],
        start: usize,
        len: usize,
    ) -> Self::Chunk<'w>;

    /// Visits a prevalidated subrange of raw component columns entity by entity.
    ///
    /// This is the entity-level counterpart to [`chunk_from_raw_parts`](Self::chunk_from_raw_parts)
    /// used by the parallel stripe runner.
    ///
    /// # Safety
    ///
    /// Every pointer must address the corresponding descriptor column, the
    /// range must be initialized and in bounds, and concurrent calls must use
    /// disjoint ranges for every mutable component access.
    #[doc(hidden)]
    unsafe fn for_each_entity_raw_parts<'w, Func>(
        component_ptrs: &[*mut u8],
        start: usize,
        len: usize,
        f: &mut Func,
    ) where
        Func: FnMut(Self::Item<'w>);

    /// Visits every initialized entity row in one matching chunk.
    ///
    /// # Safety
    ///
    /// `component_indices` must match this specification and `chunk`; the
    /// caller must also uphold the aliasing contract for all yielded items
    /// until each closure invocation returns.
    unsafe fn for_each_entity<'w, Func>(chunk: &'w Chunk, component_indices: &[u8], f: &mut Func)
    where
        Func: FnMut(Self::Item<'w>);
}

/// Marker for query specifications that never yield mutable references.
///
/// # Safety
///
/// Every access declared by the implementing [`QuerySpec`] must be read-only.
pub unsafe trait ReadOnlyQuerySpec: QuerySpec {}

unsafe impl<P: QueryParam> QuerySpec for P {
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
            0,
            chunk.entity_count,
        )
    }

    #[inline(always)]
    unsafe fn chunk_from_raw_parts<'w>(
        component_ptrs: &[*mut u8],
        start: usize,
        len: usize,
    ) -> Self::Chunk<'w> {
        P::slice_from_raw(component_ptrs[0], start, len)
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

    #[inline(always)]
    unsafe fn for_each_entity_raw_parts<'w, Func>(
        component_ptrs: &[*mut u8],
        start: usize,
        len: usize,
        f: &mut Func,
    ) where
        Func: FnMut(Self::Item<'w>),
    {
        let base = component_ptrs[0];
        for entity_index in start..start + len {
            f(P::item_from_raw(base, entity_index));
        }
    }
}

unsafe impl<P: ReadOnlyQueryParam> ReadOnlyQuerySpec for P {}

macro_rules! impl_query_spec_tuple {
    ($(($Param:ident, $base:ident, $index:tt)),+ $(,)?) => {
        unsafe impl<$($Param: QueryParam),+> QuerySpec for ($($Param,)+) {
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
                            0,
                            chunk.entity_count,
                        ),
                    )+
                )
            }

            #[inline(always)]
            unsafe fn chunk_from_raw_parts<'w>(
                component_ptrs: &[*mut u8],
                start: usize,
                len: usize,
            ) -> Self::Chunk<'w> {
                (
                    $(
                        $Param::slice_from_raw(component_ptrs[$index], start, len),
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

            #[inline(always)]
            unsafe fn for_each_entity_raw_parts<'w, Func>(
                component_ptrs: &[*mut u8],
                start: usize,
                len: usize,
                f: &mut Func,
            )
            where
                Func: FnMut(Self::Item<'w>),
            {
                $(let $base = component_ptrs[$index];)+

                for entity_index in start..start + len {
                    f((
                        $(
                            $Param::item_from_raw($base, entity_index),
                        )+
                    ));
                }
            }
        }

        unsafe impl<$($Param: ReadOnlyQueryParam),+> ReadOnlyQuerySpec for ($($Param,)+) {}
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
impl_query_spec_tuple!(
    (A, a, 0),
    (B, b, 1),
    (C, c, 2),
    (D, d, 3),
    (E, e, 4),
    (F, f, 5),
    (G, g, 6),
    (H, h, 7),
    (I, i, 8)
);
impl_query_spec_tuple!(
    (A, a, 0),
    (B, b, 1),
    (C, c, 2),
    (D, d, 3),
    (E, e, 4),
    (F, f, 5),
    (G, g, 6),
    (H, h, 7),
    (I, i, 8),
    (J, j, 9)
);
impl_query_spec_tuple!(
    (A, a, 0),
    (B, b, 1),
    (C, c, 2),
    (D, d, 3),
    (E, e, 4),
    (F, f, 5),
    (G, g, 6),
    (H, h, 7),
    (I, i, 8),
    (J, j, 9),
    (K, k, 10)
);
impl_query_spec_tuple!(
    (A, a, 0),
    (B, b, 1),
    (C, c, 2),
    (D, d, 3),
    (E, e, 4),
    (F, f, 5),
    (G, g, 6),
    (H, h, 7),
    (I, i, 8),
    (J, j, 9),
    (K, k, 10),
    (L, l, 11)
);
impl_query_spec_tuple!(
    (A, a, 0),
    (B, b, 1),
    (C, c, 2),
    (D, d, 3),
    (E, e, 4),
    (F, f, 5),
    (G, g, 6),
    (H, h, 7),
    (I, i, 8),
    (J, j, 9),
    (K, k, 10),
    (L, l, 11),
    (M, m, 12)
);
impl_query_spec_tuple!(
    (A, a, 0),
    (B, b, 1),
    (C, c, 2),
    (D, d, 3),
    (E, e, 4),
    (F, f, 5),
    (G, g, 6),
    (H, h, 7),
    (I, i, 8),
    (J, j, 9),
    (K, k, 10),
    (L, l, 11),
    (M, m, 12),
    (N, n, 13)
);
impl_query_spec_tuple!(
    (A, a, 0),
    (B, b, 1),
    (C, c, 2),
    (D, d, 3),
    (E, e, 4),
    (F, f, 5),
    (G, g, 6),
    (H, h, 7),
    (I, i, 8),
    (J, j, 9),
    (K, k, 10),
    (L, l, 11),
    (M, m, 12),
    (N, n, 13),
    (O, o, 14)
);
impl_query_spec_tuple!(
    (A, a, 0),
    (B, b, 1),
    (C, c, 2),
    (D, d, 3),
    (E, e, 4),
    (F, f, 5),
    (G, g, 6),
    (H, h, 7),
    (I, i, 8),
    (J, j, 9),
    (K, k, 10),
    (L, l, 11),
    (M, m, 12),
    (N, n, 13),
    (O, o, 14),
    (P, p, 15)
);
