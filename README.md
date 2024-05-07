# Events as entities blabla

`bevy_events_as_entities` is a simple alternative to the built in event system in bevy, each event is an entity which can have one or more components which make up an event.

## How is this different from the built in system?

All events are stored in the same `Events` (not to be confused with `bevy::ecs::event::Event`) resource which means the ordering of events are predictable even if they are of different types.

Events are entities so you can add arbitrary component to events.

## Possibly outdated example

```rust
fn main() {
    App::new()
        // Just add the plugin, no need to add every possible event.
        .add_plugins(EventPlugin::default())
        .add_systems(Update, (attack_enemy, deal_damage).chain())
        .run();
}

// We derive `Component` instead of `Event` since events are just entities with components.
#[derive(Component)]
struct Attack {
    damage: u32,
}

fn attack_enemy(
    mut commands: Commands,
    player: Query<Entity, With<Player>>,
    enemy: Query<Entity, With<Enemy>>,
) {
    // Use `Commands` to send events.
    commands.send_event((
        Attack {
            damage: 10,
        },
        // One event can have multiple components.
        Target(enemy.single()),
        Instigator(player.single()),
    ));
}

fn deal_damage(
    // `QueryEventReader` only supports read only access to components.
    // If for whatever reason you need to mutate the components of an event,
    // use `EntityEventReader` + `Query` instead.
    mut reader: QueryEventReader<(&Attack, &Target, &Instigator)>,
    mut query: Query<&mut Health>,
) {
    for (&Attack { damage }, &Target(target), &Instigator(instigator)) in reader.read() {
        info!("{instigator:?} attacked {target:?} with {damage} damage!");
        let mut health = query.get_mut(target).unwrap();
        health.value = health.value.saturating_sub(damage);
    }
}

#[derive(Component)]
struct Health {
    value: u32
}

#[derive(Component)]
struct Player;

#[derive(Component)]
struct Enemy;

fn setup(mut commands: Commands) {
    commands.spawn(Player);
    commands.spawn((Enemy, Health { value: 10 }));
}
```

See also [minimal.rs](https://github.com/atornity/bevy_events_as_entities/blob/master/examples/minimal.rs), [damage.rs](https://github.com/atornity/bevy_events_as_entities/blob/master/examples/damage.rs) and [listener.rs](https://github.com/atornity/bevy_events_as_entities/blob/master/examples/listener.rs).
