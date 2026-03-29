use crate::reflect::register_rust_type;
use core::marker::PhantomData;

pub trait QueryFilter {
    fn matches_archetype(archetype: &super::super::InternalArchetype) -> bool;
}

impl QueryFilter for () {
    #[inline(always)]
    fn matches_archetype(_: &super::super::InternalArchetype) -> bool {
        true
    }
}

pub struct With<T>(PhantomData<T>);
pub struct Without<T>(PhantomData<T>);

impl<T: 'static> QueryFilter for With<T> {
    #[inline(always)]
    fn matches_archetype(archetype: &super::super::InternalArchetype) -> bool {
        archetype.has_component(&register_rust_type::<T>())
    }
}

impl<T: 'static> QueryFilter for Without<T> {
    #[inline(always)]
    fn matches_archetype(archetype: &super::super::InternalArchetype) -> bool {
        !archetype.has_component(&register_rust_type::<T>())
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
