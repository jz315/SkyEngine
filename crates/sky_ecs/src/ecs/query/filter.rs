use super::QueryFilterSealed;
use crate::ecs::{component_type, ComponentType};
use core::marker::PhantomData;

/// Trait for compile-time archetype filters applied to queries.
///
/// Filters are composed using tuples with AND semantics:
/// `(With<A>, Without<B>)` matches archetypes that have `A` but not `B`.
///
/// This trait is sealed. Compose the built-in [`With`], [`Without`], and
/// [`Any`] filters instead of implementing it directly.
pub trait QueryFilter: QueryFilterSealed {
    #[doc(hidden)]
    const IS_TRIVIAL: bool = false;
    #[doc(hidden)]
    const IS_CONJUNCTIVE: bool = false;
    #[doc(hidden)]
    const TERM_COUNT: usize = 0;

    #[doc(hidden)]
    fn matches_archetype(archetype: &super::super::InternalArchetype) -> bool;

    /// Visits a flat conjunction of `(component, must_be_present)` terms.
    /// `false` denotes a filter with disjunctions that must use its typed
    /// evaluator. Callers must ignore terms emitted before `false`.
    #[doc(hidden)]
    fn collect_conjunctive_terms(_visitor: &mut dyn FnMut(ComponentType, bool)) -> bool {
        false
    }
}

impl QueryFilterSealed for () {}

impl QueryFilter for () {
    const IS_TRIVIAL: bool = true;
    const IS_CONJUNCTIVE: bool = true;

    #[inline(always)]
    fn matches_archetype(_: &super::super::InternalArchetype) -> bool {
        true
    }

    fn collect_conjunctive_terms(_visitor: &mut dyn FnMut(ComponentType, bool)) -> bool {
        true
    }
}

/// Includes only archetypes that contain component `T`.
///
/// Attach it with [`Query::filter`](super::Query::filter) or
/// [`QueryMut::filter`](super::QueryMut::filter).
pub struct With<T>(PhantomData<T>);

/// Excludes archetypes that contain component `T`.
///
/// Attach it with [`Query::filter`](super::Query::filter) or
/// [`QueryMut::filter`](super::QueryMut::filter).
pub struct Without<T>(PhantomData<T>);

/// Matches an archetype when any member of its filter tuple matches.
///
/// ```
/// # use sky_ecs::{Any, With, World};
/// # #[derive(Clone, Copy)] struct Position;
/// # #[derive(Clone, Copy)] struct Enemy;
/// # #[derive(Clone, Copy)] struct Boss;
/// # let mut world = World::new();
/// let query = world
///     .query::<&Position>()
///     .filter::<Any<(With<Enemy>, With<Boss>)>>();
/// ```
pub struct Any<F>(PhantomData<F>);

impl<T: 'static> QueryFilterSealed for With<T> {}

impl<T: 'static> QueryFilter for With<T> {
    const IS_CONJUNCTIVE: bool = true;
    const TERM_COUNT: usize = 1;

    #[inline(always)]
    fn matches_archetype(archetype: &super::super::InternalArchetype) -> bool {
        archetype.has_component(&component_type::<T>())
    }

    fn collect_conjunctive_terms(visitor: &mut dyn FnMut(ComponentType, bool)) -> bool {
        visitor(component_type::<T>(), true);
        true
    }
}

impl<T: 'static> QueryFilterSealed for Without<T> {}

impl<T: 'static> QueryFilter for Without<T> {
    const IS_CONJUNCTIVE: bool = true;
    const TERM_COUNT: usize = 1;

    #[inline(always)]
    fn matches_archetype(archetype: &super::super::InternalArchetype) -> bool {
        !archetype.has_component(&component_type::<T>())
    }

    fn collect_conjunctive_terms(visitor: &mut dyn FnMut(ComponentType, bool)) -> bool {
        visitor(component_type::<T>(), false);
        true
    }
}

macro_rules! impl_query_filter_tuple {
    ($($F:ident),+) => {
        impl<$($F: QueryFilter),+> QueryFilterSealed for ($($F,)+) {}

        impl<$($F: QueryFilter),+> QueryFilter for ($($F,)+) {
            const IS_CONJUNCTIVE: bool = true $(&& $F::IS_CONJUNCTIVE)+;
            const TERM_COUNT: usize = 0 $(+ $F::TERM_COUNT)+;

            #[inline(always)]
            fn matches_archetype(archetype: &super::super::InternalArchetype) -> bool {
                $($F::matches_archetype(archetype))&&+
            }

            fn collect_conjunctive_terms(
                visitor: &mut dyn FnMut(ComponentType, bool),
            ) -> bool {
                true $(&& $F::collect_conjunctive_terms(visitor))+
            }
        }
    };
}

macro_rules! impl_any_query_filter_tuple {
    ($($F:ident),+) => {
        impl<$($F: QueryFilter),+> QueryFilterSealed for Any<($($F,)+)> {}

        impl<$($F: QueryFilter),+> QueryFilter for Any<($($F,)+)> {
            #[inline(always)]
            fn matches_archetype(archetype: &super::super::InternalArchetype) -> bool {
                $($F::matches_archetype(archetype))||+
            }

            fn collect_conjunctive_terms(
                _visitor: &mut dyn FnMut(ComponentType, bool),
            ) -> bool {
                false
            }
        }
    };
}

impl_query_filter_tuple!(A, B);
impl_query_filter_tuple!(A, B, C);
impl_query_filter_tuple!(A, B, C, D);
impl_query_filter_tuple!(A, B, C, D, E);
impl_query_filter_tuple!(A, B, C, D, E, F);
impl_query_filter_tuple!(A, B, C, D, E, F, G);
impl_query_filter_tuple!(A, B, C, D, E, F, G, H);
impl_query_filter_tuple!(A, B, C, D, E, F, G, H, I);
impl_query_filter_tuple!(A, B, C, D, E, F, G, H, I, J);
impl_query_filter_tuple!(A, B, C, D, E, F, G, H, I, J, K);
impl_query_filter_tuple!(A, B, C, D, E, F, G, H, I, J, K, L);
impl_query_filter_tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M);
impl_query_filter_tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M, N);
impl_query_filter_tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O);
impl_query_filter_tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P);

impl_any_query_filter_tuple!(A, B);
impl_any_query_filter_tuple!(A, B, C);
impl_any_query_filter_tuple!(A, B, C, D);
impl_any_query_filter_tuple!(A, B, C, D, E);
impl_any_query_filter_tuple!(A, B, C, D, E, F);
impl_any_query_filter_tuple!(A, B, C, D, E, F, G);
impl_any_query_filter_tuple!(A, B, C, D, E, F, G, H);
impl_any_query_filter_tuple!(A, B, C, D, E, F, G, H, I);
impl_any_query_filter_tuple!(A, B, C, D, E, F, G, H, I, J);
impl_any_query_filter_tuple!(A, B, C, D, E, F, G, H, I, J, K);
impl_any_query_filter_tuple!(A, B, C, D, E, F, G, H, I, J, K, L);
impl_any_query_filter_tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M);
impl_any_query_filter_tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M, N);
impl_any_query_filter_tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O);
impl_any_query_filter_tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P);
