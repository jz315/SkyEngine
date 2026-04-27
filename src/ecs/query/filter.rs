use crate::ecs::component_type;
use core::marker::PhantomData;

/// Trait for compile-time archetype filters applied to queries.
///
/// Filters are composed using tuples with AND semantics:
/// `(With<A>, Without<B>)` matches archetypes that have `A` but not `B`.
pub trait QueryFilter {
    fn matches_archetype(archetype: &super::super::InternalArchetype) -> bool;
}

impl QueryFilter for () {
    #[inline(always)]
    fn matches_archetype(_: &super::super::InternalArchetype) -> bool {
        true
    }
}

/// Includes only archetypes that contain component `T`.
///
/// Used as a filter parameter in [`World::query_filtered`](super::World::query_filtered).
pub struct With<T>(PhantomData<T>);

/// Excludes archetypes that contain component `T`.
///
/// Used as a filter parameter in [`World::query_filtered`](super::World::query_filtered).
pub struct Without<T>(PhantomData<T>);

impl<T: 'static> QueryFilter for With<T> {
    #[inline(always)]
    fn matches_archetype(archetype: &super::super::InternalArchetype) -> bool {
        archetype.has_component(&component_type::<T>())
    }
}

impl<T: 'static> QueryFilter for Without<T> {
    #[inline(always)]
    fn matches_archetype(archetype: &super::super::InternalArchetype) -> bool {
        !archetype.has_component(&component_type::<T>())
    }
}

macro_rules! impl_query_filter_tuple {
    ($($F:ident),+) => {
        impl<$($F: QueryFilter),+> QueryFilter for ($($F,)+) {
            #[inline(always)]
            fn matches_archetype(archetype: &super::super::InternalArchetype) -> bool {
                $($F::matches_archetype(archetype))&&+
            }
        }
    };
}

impl_query_filter_tuple!(A, B);
impl_query_filter_tuple!(A, B, C);
impl_query_filter_tuple!(A, B, C, D);
