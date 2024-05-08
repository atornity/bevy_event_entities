use bevy::prelude::*;
use bevy_event_entities::{event_listener::Target, prelude::*};

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, EventPlugin::default()))
        .add_systems(Startup, setup)
        .add_systems(Update, attack_enemy)
        // `process_kill` have to run after `process_attack` since it's referencing the entity for the `Attack` event
        // and event entities are despawned every update.
        .add_systems(
            PostUpdate,
            (block_attack, process_attack, defy_death, process_kill).chain(),
        )
        .run()
}

#[derive(Component)]
struct Enemy;

#[derive(Component)]
struct Health {
    value: u32,
    max: u32,
}

impl Health {
    fn new(health: u32) -> Self {
        Self {
            value: health,
            max: health,
        }
    }
}

#[derive(Component)]
struct Armor(u32);

#[derive(Component)]
struct DeathDefiance;

fn enemy() -> impl Bundle {
    (Enemy, Health::new(5), Armor(2), DeathDefiance)
}

#[derive(Component, Debug)]
struct Attack {
    damage: u32,
}

#[derive(Component)]
struct Kill {
    // Events can reference other events.
    attack: Option<Entity>,
}

fn setup(mut commands: Commands) {
    commands.spawn(enemy());
}

fn attack_enemy(
    mut commands: Commands,
    enemy: Query<Entity, With<Enemy>>,
    key: Res<ButtonInput<KeyCode>>,
) {
    if key.just_pressed(KeyCode::Space) {
        // Events are sent with `Commands` instead of an `EventWriter`.
        commands.send_event((Attack { damage: 1 }, Target(enemy.single())));
    }
}

fn process_attack(
    mut commands: Commands,
    mut events: QueryEventReader<(Entity, &Attack, &Target)>,
    mut query: Query<&mut Health>,
) {
    for (attack, &Attack { damage }, &Target(target)) in events.read() {
        let Ok(mut health) = query.get_mut(target) else {
            continue;
        };
        let new_health = health.value.saturating_sub(damage);
        info!(
            "{target:?} was attacked with {damage} damage, health = {} - {damage} = {new_health}",
            health.value
        );
        health.value = new_health;
        if health.value == 0 {
            commands.send_event((
                Kill {
                    attack: Some(attack),
                },
                Target(target),
            ));
        }
    }
}

fn process_kill(
    mut commands: Commands,
    mut events: QueryEventReader<(&Kill, &Target)>,
    attacks: Query<&Attack>,
) {
    for (&Kill { attack }, &Target(target)) in events.read() {
        match attack.and_then(|entity| attacks.get(entity).ok()) {
            Some(attack) => {
                info!("{target:?} was killed with {attack:?}");
            }
            None => {
                info!("{target:?} was killed");
            }
        }

        // respawn enemy
        commands.entity(target).despawn();
        commands.spawn(enemy());
    }
}

fn block_attack(
    mut commands: Commands,
    mut events: QueryEventReader<(Entity, &Target), With<Attack>>,
    // Since the query in `QueryEventReader` is read-only, we must specify a new query to modify components in an event.
    mut attacks: Query<&mut Attack>,
    mut query: Query<&mut Armor>,
) {
    for (event, &Target(target)) in events.read() {
        let mut attack = attacks.get_mut(event).unwrap();
        let Ok(mut armor) = query.get_mut(target) else {
            continue;
        };
        let new_armor = armor.0.saturating_sub(attack.damage);
        info!(
            "{target:?} blocked the attack, armor = {} - {} = {new_armor}",
            armor.0, attack.damage
        );
        attack.damage = attack.damage.saturating_sub(armor.0);
        armor.0 = new_armor;

        if attack.damage == 0 {
            commands.entity(event).despawn();
        }
        if armor.0 == 0 {
            commands.entity(target).remove::<Armor>();
        }
    }
}

fn defy_death(
    mut commands: Commands,
    mut events: QueryEventReader<(Entity, &Target), With<Kill>>,
    mut query: Query<&mut Health, With<DeathDefiance>>,
) {
    for (kill, &Target(target)) in events.read() {
        if let Ok(mut health) = query.get_mut(target) {
            health.value = health.max;
            info!("{target:?} defied death, health = {}", health.value);
            commands.entity(kill).despawn();
            commands.entity(target).remove::<DeathDefiance>();
        }
    }
}
