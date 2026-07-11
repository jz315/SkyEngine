use sky_ecs::dynamic::{DynamicBundle, WorldDynamicExt};
use sky_ecs::{PreparedQuery, QueryData, World};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

struct CountingAllocator;

static TRACKING: AtomicBool = AtomicBool::new(false);
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if TRACKING.load(Ordering::Relaxed) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if TRACKING.load(Ordering::Relaxed) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if TRACKING.load(Ordering::Relaxed) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

macro_rules! markers {
    ($($name:ident),+ $(,)?) => {
        $(#[derive(Clone, Copy, Default)] struct $name;)+
    };
}

markers!(
    BaseA, BaseB, BaseC, BaseD, BaseE, BaseF, BaseG, BaseH, BaseI, BaseJ, BaseK, BaseL, BaseM,
    BaseN, BaseO, BaseP, ShapeA, ShapeB, ShapeC, ShapeD, ShapeE, ShapeF,
);

#[derive(QueryData)]
#[allow(dead_code)]
struct WideQuery<'w> {
    a: &'w BaseA,
    b: &'w BaseB,
    c: &'w BaseC,
    d: &'w BaseD,
    e: &'w BaseE,
    component_f: &'w BaseF,
    g: &'w BaseG,
    h: &'w BaseH,
    i: &'w BaseI,
    j: &'w BaseJ,
    k: &'w BaseK,
    l: &'w BaseL,
    m: &'w BaseM,
    n: &'w BaseN,
    o: &'w BaseO,
    p: &'w BaseP,
}

fn world_with_matching_shapes(shape_count: usize) -> World {
    assert!(shape_count <= 64);
    let mut world = World::new();
    for mask in 0..shape_count {
        let mut bundle = DynamicBundle::new()
            .with(BaseA)
            .with(BaseB)
            .with(BaseC)
            .with(BaseD)
            .with(BaseE)
            .with(BaseF)
            .with(BaseG)
            .with(BaseH)
            .with(BaseI)
            .with(BaseJ)
            .with(BaseK)
            .with(BaseL)
            .with(BaseM)
            .with(BaseN)
            .with(BaseO)
            .with(BaseP);
        if mask & 1 != 0 {
            bundle = bundle.with(ShapeA);
        }
        if mask & 2 != 0 {
            bundle = bundle.with(ShapeB);
        }
        if mask & 4 != 0 {
            bundle = bundle.with(ShapeC);
        }
        if mask & 8 != 0 {
            bundle = bundle.with(ShapeD);
        }
        if mask & 16 != 0 {
            bundle = bundle.with(ShapeE);
        }
        if mask & 32 != 0 {
            bundle = bundle.with(ShapeF);
        }
        world.spawn_dynamic(bundle).unwrap();
    }
    world
}

fn first_prepare_allocations(shape_count: usize) -> usize {
    let world = world_with_matching_shapes(shape_count);
    let mut query = PreparedQuery::<WideQuery>::new();

    ALLOCATIONS.store(0, Ordering::Relaxed);
    TRACKING.store(true, Ordering::Release);
    let matched = query.count(&world);
    TRACKING.store(false, Ordering::Release);

    assert_eq!(matched, shape_count);
    ALLOCATIONS.load(Ordering::Relaxed)
}

#[test]
fn wide_query_column_maps_do_not_allocate_per_matching_archetype() {
    let eight_shapes = first_prepare_allocations(8);
    let sixty_four_shapes = first_prepare_allocations(64);

    assert!(
        sixty_four_shapes <= eight_shapes + 8,
        "allocation count scaled with matching archetypes: 8 shapes={eight_shapes}, 64 shapes={sixty_four_shapes}"
    );
}
