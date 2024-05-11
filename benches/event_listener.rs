use bevy::prelude::*;
use bevy_event_entities::{
    event_listener::{
        event_listener_system_configs, AddCallbackExt, Listenable, Listener, On, Target,
    },
    send_event, EventEntities,
};

fn main() {
    divan::main();
}

#[derive(Component, Listenable)]
struct MyEvent {
    num: usize,
}

fn setup() -> (World, Schedule) {
    let mut world = World::new();
    world.init_resource::<EventEntities>();
    let mut schedule = Schedule::default();
    schedule.add_systems(event_listener_system_configs());
    (world, schedule)
}

#[divan::bench(args = [
    (false, 100, 100), (true, 100, 100),
    (false, 1_000, 1), (true, 1_000, 1),
    (false, 1, 1_000), (true, 1, 1_000),
])]
fn nested((switch, depth, n): (bool, usize, usize)) {
    let callback = |input: Listener<&MyEvent>| {
        assert_eq!(69, input.event().num);
    };

    let (mut world, mut schedule) = setup();

    let mut entity = world.spawn_empty();
    for _ in 0..n {
        entity.add_callback(On::<MyEvent>::run(callback));
    }
    let mut entity = entity.id();
    for _ in 0..depth {
        let mut child = world.spawn_empty();
        for _ in 0..n {
            child.add_callback(On::<MyEvent>::run(callback));
        }
        let child = child.id();
        world.entity_mut(entity).add_child(child);
        entity = child;
    }

    send_event(&mut world, MyEvent { num: 69 }).insert(Target(entity));

    if switch {
        schedule.run(&mut world);
    }
}
