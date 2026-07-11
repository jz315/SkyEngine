use sky_ecs::stage::{PostUpdate, StageLabel, Update};
use sky_ecs::World;

#[derive(StageLabel)]
struct BetweenUpdateAndPost;

#[derive(Default)]
struct Trace(Vec<&'static str>);

#[test]
fn derived_stage_labels_preserve_explicit_stage_order() {
    let mut world = World::new();
    world.insert_resource(Trace::default());
    world.stage(Update).add_exclusive(|world: &mut World| {
        world.get_resource_mut::<Trace>().unwrap().0.push("update");
    });
    world
        .insert_stage_after(Update, BetweenUpdateAndPost)
        .unwrap()
        .add_exclusive(|world: &mut World| {
            world.get_resource_mut::<Trace>().unwrap().0.push("between");
        });
    world.stage(PostUpdate).add_exclusive(|world: &mut World| {
        world.get_resource_mut::<Trace>().unwrap().0.push("post");
    });

    world.tick_with_delta(0.016).unwrap();

    assert_eq!(
        world.get_resource::<Trace>().unwrap().0,
        vec!["update", "between", "post"]
    );
}
