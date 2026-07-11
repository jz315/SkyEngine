use sky_ecs::{Any, QueryData, With, World};

#[derive(Clone, Copy, Debug, PartialEq)]
struct Position(f32);

#[derive(Clone, Copy)]
struct Velocity(f32);

#[derive(Clone, Copy)]
struct Enemy;

#[derive(Clone, Copy)]
struct Friendly;

#[derive(QueryData)]
struct Movement<'w> {
    position: &'w mut Position,
    velocity: &'w Velocity,
}

#[derive(QueryData)]
struct ReadMovement<'w> {
    position: &'w Position,
    velocity: Option<&'w Velocity>,
}

#[test]
fn named_query_data_supports_read_and_mutable_items() {
    let mut world = World::new();
    world.spawn((Position(1.0), Velocity(2.0)));
    world.spawn((Position(10.0),));

    world
        .query_mut::<Movement>()
        .for_each(|item| item.position.0 += item.velocity.0);

    let mut values = Vec::new();
    world.query::<ReadMovement>().for_each(|item| {
        values.push((item.position.0, item.velocity.map(|velocity| velocity.0)));
    });
    values.sort_by(|left, right| left.0.total_cmp(&right.0));

    assert_eq!(values, vec![(3.0, Some(2.0)), (10.0, None)]);
}

#[test]
fn named_query_data_uses_the_same_parallel_chunk_path() {
    let mut world = World::new();
    for _ in 0..32 {
        world.spawn((Position(1.0), Velocity(2.0)));
    }

    world
        .query_mut::<Movement>()
        .par_for_each_chunk(|(positions, velocities)| {
            for (position, velocity) in positions.iter_mut().zip(velocities) {
                position.0 += velocity.0;
            }
        });

    world
        .query::<&Position>()
        .for_each(|position| assert_eq!(*position, Position(3.0)));
}

#[test]
fn named_query_data_uses_the_same_parallel_entity_path() {
    let mut world = World::new();
    for _ in 0..32 {
        world.spawn((Position(1.0), Velocity(2.0)));
    }

    world
        .query_mut::<Movement>()
        .par_for_each(|item| item.position.0 += item.velocity.0);

    world
        .query::<&Position>()
        .for_each(|position| assert_eq!(*position, Position(3.0)));
}

#[test]
fn any_filter_composes_with_named_query_data() {
    let mut world = World::new();
    world.spawn((Position(1.0), Velocity(1.0), Enemy));
    world.spawn((Position(2.0), Velocity(1.0), Friendly));
    world.spawn((Position(3.0), Velocity(1.0)));

    let query = world
        .query::<ReadMovement>()
        .filter::<Any<(With<Enemy>, With<Friendly>)>>();

    assert_eq!(query.count(), 2);
}

macro_rules! numbered_components {
    ($($name:ident),+ $(,)?) => {
        $(
            #[derive(Clone, Copy)]
            struct $name(u8);
        )+
    };
}

numbered_components!(C0, C1, C2, C3, C4, C5, C6, C7, C8, C9, C10, C11, C12, C13, C14, C15);

#[derive(QueryData)]
struct WideQuery<'w> {
    c0: &'w C0,
    c1: &'w C1,
    c2: &'w C2,
    c3: &'w C3,
    c4: &'w C4,
    c5: &'w C5,
    c6: &'w C6,
    c7: &'w C7,
    c8: &'w C8,
    c9: &'w C9,
    c10: &'w C10,
    c11: &'w C11,
    c12: &'w C12,
    c13: &'w C13,
    c14: &'w C14,
    c15: &'w C15,
}

#[test]
fn query_data_and_filters_support_sixteen_components() {
    let mut world = World::new();
    let entity = world.spawn((C0(0),));
    assert!(world.insert(entity, C1(1)));
    assert!(world.insert(entity, C2(2)));
    assert!(world.insert(entity, C3(3)));
    assert!(world.insert(entity, C4(4)));
    assert!(world.insert(entity, C5(5)));
    assert!(world.insert(entity, C6(6)));
    assert!(world.insert(entity, C7(7)));
    assert!(world.insert(entity, C8(8)));
    assert!(world.insert(entity, C9(9)));
    assert!(world.insert(entity, C10(10)));
    assert!(world.insert(entity, C11(11)));
    assert!(world.insert(entity, C12(12)));
    assert!(world.insert(entity, C13(13)));
    assert!(world.insert(entity, C14(14)));
    assert!(world.insert(entity, C15(15)));

    let query = world.query::<WideQuery>().filter::<(
        With<C0>,
        With<C1>,
        With<C2>,
        With<C3>,
        With<C4>,
        With<C5>,
        With<C6>,
        With<C7>,
        With<C8>,
        With<C9>,
        With<C10>,
        With<C11>,
        With<C12>,
        With<C13>,
        With<C14>,
        With<C15>,
    )>();

    let mut sum = 0u32;
    query.for_each(|item| {
        sum = u32::from(item.c0.0)
            + u32::from(item.c1.0)
            + u32::from(item.c2.0)
            + u32::from(item.c3.0)
            + u32::from(item.c4.0)
            + u32::from(item.c5.0)
            + u32::from(item.c6.0)
            + u32::from(item.c7.0)
            + u32::from(item.c8.0)
            + u32::from(item.c9.0)
            + u32::from(item.c10.0)
            + u32::from(item.c11.0)
            + u32::from(item.c12.0)
            + u32::from(item.c13.0)
            + u32::from(item.c14.0)
            + u32::from(item.c15.0);
    });

    assert_eq!(query.count(), 1);
    assert_eq!(sum, 120);
}
