# BEE 🐝

`bevy_event_entities` is a simple alternative to the built in event system in [bevy](https://www.bevyengine.org) (not a replacement), each event is an entity which can have one or more components which together make up an event.

## How is this different from the built in system?

All events are stored in the same `EventEntities` resource which means the ordering of events are predictable even if they are of different types.

Events are just entities so you can add arbitrary components to events.

`bevy_event_entities` is a lot slower which may be noticable when sending a bunch (thousands) of events every frame.

## Possibly outdated example

```rust
fn main() {
    App::new()
        // Just add the plugin, no need to add every possible event.
        .add_plugins(EventPlugin::default())
        .add_systems(Update, (attack_enemy, deal_damage, kill_stuff).chain())
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn((Player, Damage(20)));
    commands.spawn((Enemy, Health(10)));
}

fn attack_enemy(
    mut commands: Commands,
    player: Query<(Entity, Damage), With<Player>>,
    enemy: Query<Entity, With<Enemy>>,
) {
    let (player, &Damage(damage)) = player.single();
    let enemy = enemy.single();

    // Use `Commands` to send events.
    commands.send_event((
        Damage(damage),
        // One event can have multiple components.
        Target(enemy),
        Instigator(player),
    ));
}

fn deal_damage(
    mut commands: Commands,
    // `QueryEventReader` only supports read only access to components.
    // If for whatever reason you need to mutate the components of an event,
    // use `EntityEventReader` + `Query` instead.
    mut events: QueryEventReader<(&Damage, &Target, &Instigator)>,
    mut query: Query<&mut Health>,
) {
    for (&Damage(damage), &Target(target), &Instigator(instigator)) in events.read() {
        info!("{instigator:?} attacked {target:?} with {damage} damage!");
        let mut health = query.get_mut(target).unwrap();
        health.0 = health.0.saturating_sub(damage);
        if health.0 == 0 {
            commands.send_event((
                Kill,
                Target(target),
                Instigator(instigator),
            ));
        }
    }
}

fn kill_stuff(
    mut commands: Commands,
    mut events: QueryEventReader<(&Target, &Instigator), With<Kill>>,
) {
    for (&Target(target), &Instigator(instigator)) in events.read() {
        info!("{instigator:?} killed {target:?}!");
        commands.entity(target).despawn();
    }
}

// We derive `Component` instead of `Event` since events are just entities with components.
#[derive(Component)]
struct Kill;

#[derive(Component)]
struct Health(u32)

#[derive(Component)]
struct Damage(u32);

#[derive(Component)]
struct Player;

#[derive(Component)]
struct Enemy;
```

See also [minimal.rs](https://github.com/atornity/bevy_events_as_entities/blob/master/examples/minimal.rs), [damage.rs](https://github.com/atornity/bevy_events_as_entities/blob/master/examples/damage.rs) and [listener.rs](https://github.com/atornity/bevy_events_as_entities/blob/master/examples/listener.rs).

## Event listener

This crate also offers an event listener implementation (think [bevy_eventlistener](https://github.com/aevyrie/bevy_eventlistener) made to work with this crate).
