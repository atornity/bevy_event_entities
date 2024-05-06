# Events as entities

`bevy_events_as_entities` is an alternative to the built in event system in bevy, each event is an entity which can have one or more components which make up an event.

All events are stored in the same `Events` resource which means the ordering of events are predictable even if they are of different types. This is a known limitation of the built in event system as of bevy 0.13.

## Benefits

- mutable event state
- consistent ordering

## Beep Boop

```rust
fn main() {
    App::new()
        .add_plugins(EventPlugin::default())
        .add_systems(Update, (attack_enemy, deal_damage).chain())
        .run();
}

// we derive `Component` instead of `Event` since events are just entities with components.
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
```

A lot of the code in this repo was copy-pasted from the bevy repo, simply replacing the event data with `Entity`.
