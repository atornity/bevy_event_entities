//! Thank's so much to [aevyrie](https://github.com/aevyrie/bevy_eventlistener/blob/main/examples/minimal.rs), from whom I stole this example :3
use bevy::prelude::*;
use bevy_events_as_entities::{
    event_listener::{EventInput, EventListenerPlugin, On, SendEntityEventExt},
    prelude::*,
};
use rand::{seq::IteratorRandom, thread_rng};

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins,
            EventPlugin::default(),
            EventListenerPlugin::<Attack>::default(),
        ))
        .add_systems(Startup, setup)
        .add_systems(Update, damage_random_armor_or_player)
        .run()
}

#[derive(Component)]
struct Attack {
    damage: u32,
}

#[derive(Component)]
struct Health(u32);

#[derive(Component)]
struct Player;

#[derive(Component)]
struct Armor;

fn setup(mut commands: Commands) {
    commands
        .spawn((
            Player,
            Name::new("Goblin"),
            Health(10),
            On::<Attack>::run(block_or_take_damage),
        ))
        .with_children(|parent| {
            parent.spawn((
                Armor,
                Name::new("Helmet"),
                Health(2),
                On::<Attack>::run(block_or_take_damage),
            ));
            parent.spawn((
                Armor,
                Name::new("Shirt"),
                Health(5),
                On::<Attack>::run(block_or_take_damage),
            ));
            parent.spawn((
                Armor,
                Name::new("Socks"),
                Health(2),
                On::<Attack>::run(block_or_take_damage),
            ));
        });
}

fn block_or_take_damage(
    mut commands: Commands,
    mut attack_input: EventInput<&mut Attack>,
    mut health: Query<(&mut Health, &Name)>,
) {
    let (mut health, name) = health.get_mut(attack_input.target()).unwrap();

    let damage = attack_input.get().damage;
    let new_health = health.0.saturating_sub(damage);
    let new_damage = damage.saturating_sub(health.0);
    info!(
        "attacked {} with {} damage, health = {} - {} = {}",
        name, damage, health.0, damage, new_health
    );

    match new_health == 0 {
        true => {
            info!("killed {name}");
            commands.entity(attack_input.target()).despawn()
        }
        false => health.0 = new_health,
    }

    match new_damage == 0 {
        true => commands.entity(attack_input.event()).despawn(),
        false => attack_input.get_mut().damage = new_damage,
    }
}

fn damage_random_armor_or_player(
    mut commands: Commands,
    armor: Query<Entity, With<Armor>>,
    player: Query<Entity, With<Player>>,
    key: Res<ButtonInput<KeyCode>>,
) {
    if key.just_pressed(KeyCode::Space) {
        let mut rng = thread_rng();
        let target = armor
            .iter()
            .choose(&mut rng)
            .unwrap_or_else(|| player.single());

        commands.entity(target).send_event(Attack { damage: 3 });
    }
}
