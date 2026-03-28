use cgmath::{Matrix4, Rad, SquareMatrix, Transform as _, Vector3};
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use hecs::World as HecsWorld;
use sky_engine::ecs::{create_archetype, PreparedQuery as SkyPreparedQuery, World as SkyWorld};
use std::ptr;

const SIMPLE_ENTITY_COUNT: usize = 10_000;
const FRAGMENTED_VARIANT_COUNT: usize = 26;
const FRAGMENTED_ENTITIES_PER_VARIANT: usize = 20;
const HEAVY_ENTITY_COUNT: usize = 1_000;
const HEAVY_INVERT_COUNT: usize = 100;

#[derive(Clone, Copy)]
struct TransformComponent(Matrix4<f32>);

#[derive(Clone, Copy)]
struct PositionComponent(Vector3<f32>);

#[derive(Clone, Copy)]
struct RotationComponent(Vector3<f32>);

#[derive(Clone, Copy)]
struct VelocityComponent(Vector3<f32>);

#[derive(Clone, Copy)]
struct DataComponent(f32);

macro_rules! define_fragment_tags {
    ($($name:ident),+ $(,)?) => {
        $(
            #[derive(Clone, Copy)]
            struct $name(f32);
        )+
    };
}

define_fragment_tags!(
    A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z
);

fn suite_transform() -> TransformComponent {
    TransformComponent(Matrix4::from_scale(1.0))
}

fn suite_position() -> PositionComponent {
    PositionComponent(Vector3::unit_x())
}

fn suite_rotation() -> RotationComponent {
    RotationComponent(Vector3::unit_x())
}

fn suite_velocity() -> VelocityComponent {
    VelocityComponent(Vector3::unit_x())
}

fn heavy_matrix() -> Matrix4<f32> {
    Matrix4::<f32>::from_angle_x(Rad(1.2))
}

fn data_archetype<T: 'static>() -> sky_engine::ecs::Archetype {
    create_archetype()
        .add_rust_component::<T>()
        .add_rust_component::<DataComponent>()
        .build()
}

fn transform_archetype() -> sky_engine::ecs::Archetype {
    create_archetype()
        .add_rust_component::<TransformComponent>()
        .add_rust_component::<PositionComponent>()
        .add_rust_component::<RotationComponent>()
        .add_rust_component::<VelocityComponent>()
        .build()
}

fn heavy_archetype() -> sky_engine::ecs::Archetype {
    create_archetype()
        .add_rust_component::<Matrix4<f32>>()
        .add_rust_component::<PositionComponent>()
        .add_rust_component::<RotationComponent>()
        .add_rust_component::<VelocityComponent>()
        .build()
}

fn archetype_component_index<T: 'static>(archetype: sky_engine::ecs::Archetype) -> usize {
    let ty = sky_engine::reflect::register_rust_type::<T>();
    archetype.query_component_index(&ty).unwrap()
}

struct SkySimpleInsertPlan {
    archetype: sky_engine::ecs::Archetype,
    transform_index: usize,
    position_index: usize,
    rotation_index: usize,
    velocity_index: usize,
}

impl SkySimpleInsertPlan {
    fn new() -> Self {
        let archetype = transform_archetype();

        Self {
            archetype,
            transform_index: archetype_component_index::<TransformComponent>(archetype),
            position_index: archetype_component_index::<PositionComponent>(archetype),
            rotation_index: archetype_component_index::<RotationComponent>(archetype),
            velocity_index: archetype_component_index::<VelocityComponent>(archetype),
        }
    }

    fn spawn(&self, world: &mut SkyWorld) {
        world.add_entity(self.archetype);

        let data = world.data.last_mut().unwrap();
        let chunk = data.chunks.last_mut().unwrap();
        let entity_index = chunk.entity_count - 1;

        unsafe {
            ptr::write(
                chunk.component_ptr(self.transform_index, entity_index) as *mut TransformComponent,
                suite_transform(),
            );
            ptr::write(
                chunk.component_ptr(self.position_index, entity_index) as *mut PositionComponent,
                suite_position(),
            );
            ptr::write(
                chunk.component_ptr(self.rotation_index, entity_index) as *mut RotationComponent,
                suite_rotation(),
            );
            ptr::write(
                chunk.component_ptr(self.velocity_index, entity_index) as *mut VelocityComponent,
                suite_velocity(),
            );
        }
    }
}

struct SkySimpleIter {
    world: SkyWorld,
    query: SkyPreparedQuery<(&'static mut PositionComponent, &'static VelocityComponent)>,
}

impl SkySimpleIter {
    fn new() -> Self {
        let archetype = transform_archetype();
        let mut world = SkyWorld::new();

        for _ in 0..SIMPLE_ENTITY_COUNT {
            world.add_entity(archetype);
        }

        let mut init = world.query::<(
            &mut TransformComponent,
            &mut PositionComponent,
            &mut RotationComponent,
            &mut VelocityComponent,
        )>();
        init.for_each(&world, |(transform, position, rotation, velocity)| {
            *transform = suite_transform();
            *position = suite_position();
            *rotation = suite_rotation();
            *velocity = suite_velocity();
        });

        Self {
            world,
            query: SkyPreparedQuery::new(),
        }
    }

    fn run(&mut self) {
        self.query.for_each_chunk(&self.world, |(positions, velocities)| {
            for (position, velocity) in positions.iter_mut().zip(velocities.iter()) {
                position.0 += velocity.0;
            }
        });
    }
}

struct HecsSimpleIter(HecsWorld);

impl HecsSimpleIter {
    fn new() -> Self {
        let mut world = HecsWorld::new();
        world.spawn_batch((0..SIMPLE_ENTITY_COUNT).map(|_| {
            (
                suite_transform(),
                suite_position(),
                suite_rotation(),
                suite_velocity(),
            )
        }));

        Self(world)
    }

    fn run(&mut self) {
        for (_, (velocity, position)) in self
            .0
            .query_mut::<(&VelocityComponent, &mut PositionComponent)>()
        {
            position.0 += velocity.0;
        }
    }
}

struct SkyFragmentedIter {
    world: SkyWorld,
    query: SkyPreparedQuery<&'static mut DataComponent>,
}

impl SkyFragmentedIter {
    fn new() -> Self {
        let mut world = SkyWorld::new();

        macro_rules! add_variant {
            ($tag:ty) => {{
                let archetype = data_archetype::<$tag>();
                for _ in 0..FRAGMENTED_ENTITIES_PER_VARIANT {
                    world.add_entity(archetype);
                }
            }};
        }

        add_variant!(A);
        add_variant!(B);
        add_variant!(C);
        add_variant!(D);
        add_variant!(E);
        add_variant!(F);
        add_variant!(G);
        add_variant!(H);
        add_variant!(I);
        add_variant!(J);
        add_variant!(K);
        add_variant!(L);
        add_variant!(M);
        add_variant!(N);
        add_variant!(O);
        add_variant!(P);
        add_variant!(Q);
        add_variant!(R);
        add_variant!(S);
        add_variant!(T);
        add_variant!(U);
        add_variant!(V);
        add_variant!(W);
        add_variant!(X);
        add_variant!(Y);
        add_variant!(Z);

        let mut init = world.query::<&mut DataComponent>();
        init.for_each(&world, |data| {
            data.0 = 1.0;
        });

        Self {
            world,
            query: SkyPreparedQuery::new(),
        }
    }

    fn run(&mut self) {
        self.query.for_each(&self.world, |data| {
            data.0 *= 2.0;
        });
    }
}

struct HecsFragmentedIter(HecsWorld);

impl HecsFragmentedIter {
    fn new() -> Self {
        let mut world = HecsWorld::default();

        macro_rules! create_entities {
            ($world:ident; $( $variant:ident ),* $(,)?) => {
                $(
                    $world.spawn_batch((0..FRAGMENTED_ENTITIES_PER_VARIANT).map(|_| {
                        ($variant(0.0), DataComponent(1.0))
                    }));
                )*
            };
        }

        create_entities!(world; A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z);

        Self(world)
    }

    fn run(&mut self) {
        for (_, data) in self.0.query_mut::<&mut DataComponent>() {
            data.0 *= 2.0;
        }
    }
}

struct SkyHeavyCompute {
    world: SkyWorld,
    query: SkyPreparedQuery<(&'static mut PositionComponent, &'static mut Matrix4<f32>)>,
}

impl SkyHeavyCompute {
    fn new() -> Self {
        let archetype = heavy_archetype();
        let mut world = SkyWorld::new();

        for _ in 0..HEAVY_ENTITY_COUNT {
            world.add_entity(archetype);
        }

        let mut init = world.query::<(
            &mut Matrix4<f32>,
            &mut PositionComponent,
            &mut RotationComponent,
            &mut VelocityComponent,
        )>();
        init.for_each(&world, |(matrix, position, rotation, velocity)| {
            *matrix = heavy_matrix();
            *position = suite_position();
            *rotation = suite_rotation();
            *velocity = suite_velocity();
        });

        Self {
            world,
            query: SkyPreparedQuery::new(),
        }
    }

    fn run(&mut self) {
        self.query.for_each_chunk(&self.world, |(positions, matrices)| {
            for (position, matrix) in positions.iter_mut().zip(matrices.iter_mut()) {
                for _ in 0..HEAVY_INVERT_COUNT {
                    *matrix = matrix.invert().unwrap();
                }
                position.0 = matrix.transform_vector(position.0);
            }
        });
    }
}

struct HecsHeavyCompute(HecsWorld);

impl HecsHeavyCompute {
    fn new() -> Self {
        let mut world = HecsWorld::default();
        world.spawn_batch((0..HEAVY_ENTITY_COUNT).map(|_| {
            (
                heavy_matrix(),
                suite_position(),
                suite_rotation(),
                suite_velocity(),
            )
        }));

        Self(world)
    }

    fn run(&mut self) {
        for (_, (position, matrix)) in self
            .0
            .query_mut::<(&mut PositionComponent, &mut Matrix4<f32>)>()
        {
            for _ in 0..HEAVY_INVERT_COUNT {
                *matrix = matrix.invert().unwrap();
            }
            position.0 = matrix.transform_vector(position.0);
        }
    }
}

fn bench_simple_insert(c: &mut Criterion) {
    let plan = SkySimpleInsertPlan::new();
    let mut group = c.benchmark_group("simple_insert");

    group.bench_function("sky", |b| {
        b.iter(|| {
            let mut world = SkyWorld::new();
            for _ in 0..SIMPLE_ENTITY_COUNT {
                plan.spawn(&mut world);
            }
            black_box(world);
        });
    });

    group.bench_function("hecs", |b| {
        b.iter(|| {
            let mut world = HecsWorld::new();
            world.spawn_batch((0..SIMPLE_ENTITY_COUNT).map(|_| {
                (
                    suite_transform(),
                    suite_position(),
                    suite_rotation(),
                    suite_velocity(),
                )
            }));
            black_box(&world);
        });
    });

    group.finish();
}

fn bench_simple_iter(c: &mut Criterion) {
    let mut sky = SkySimpleIter::new();
    let mut hecs = HecsSimpleIter::new();
    let mut group = c.benchmark_group("simple_iter");

    group.bench_function("sky", |b| b.iter(|| sky.run()));
    group.bench_function("hecs", |b| b.iter(|| hecs.run()));

    group.finish();
}

fn bench_fragmented_iter(c: &mut Criterion) {
    debug_assert_eq!(FRAGMENTED_VARIANT_COUNT, 26);

    let mut sky = SkyFragmentedIter::new();
    let mut hecs = HecsFragmentedIter::new();
    let mut group = c.benchmark_group("fragmented_iter");

    group.bench_function("sky", |b| b.iter(|| sky.run()));
    group.bench_function("hecs", |b| b.iter(|| hecs.run()));

    group.finish();
}

fn bench_heavy_compute(c: &mut Criterion) {
    let mut sky = SkyHeavyCompute::new();
    let mut hecs = HecsHeavyCompute::new();
    let mut group = c.benchmark_group("heavy_compute");

    group.bench_function("sky", |b| b.iter(|| sky.run()));
    group.bench_function("hecs", |b| b.iter(|| hecs.run()));

    group.finish();
}

criterion_group!(
    suite_benches,
    bench_simple_insert,
    bench_simple_iter,
    bench_fragmented_iter,
    bench_heavy_compute,
);
criterion_main!(suite_benches);
