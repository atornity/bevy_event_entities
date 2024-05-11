//! Thank's so much to [aevyrie](https://github.com/aevyrie/bevy_eventlistener/blob/main/examples/minimal.rs), from whom I stole this example :3
use bevy::prelude::*;
use bevy_event_entities::{
    event_listener::{
        AddCallbackExt, EventListenerPlugin, Listenable, Listener, SendEntityEventExt,
    },
    prelude::*,
};
use rand::{seq::IteratorRandom, thread_rng};

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins,
            EventPlugin::default(),
            EventListenerPlugin::default(),
        ))
        .add_systems(Startup, setup)
        .add_systems(Update, damage_random_armor_or_player)
        .run()
}

#[derive(Component, Listenable)]
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
    // global callback why not
    commands.add_callback::<Attack, _>(|input: Listener| {
        if input.is_propagated() {
            return;
        }
        warn!("whoa whoa whoa now, it seems a lot like someone was attacked over here");
    });

    commands
        .spawn((Player, Name::new("Goblin"), Health(10)))
        .entity_callback::<Attack, _>(block_or_take_damage)
        .with_children(|parent| {
            parent
                .spawn((Armor, Name::new("Helmet"), Health(2)))
                .entity_callback::<Attack, _>(block_or_take_damage);

            parent
                .spawn((Armor, Name::new("Shirt"), Health(5)))
                .entity_callback::<Attack, _>(block_or_take_damage);
            parent
                .spawn((Armor, Name::new("Socks"), Health(2)))
                .entity_callback::<Attack, _>(block_or_take_damage);
        });
}

fn block_or_take_damage(
    mut commands: Commands,
    mut input: Listener<(Entity, &mut Attack, &Target)>,
    mut health: Query<(&mut Health, &Name)>,
) {
    let (event, mut attack, &Target(target)) = input.event_mut();
    let (mut health, name) = health.get_mut(target).unwrap();

    let new_health = health.0.saturating_sub(attack.damage);
    let new_damage = attack.damage.saturating_sub(health.0);
    info!(
        "attacked {} with {} damage, health: {} -> {}",
        name, attack.damage, health.0, new_health
    );

    match new_health == 0 {
        true => {
            info!("killed {name}");
            commands.entity(target).despawn_recursive()
        }
        false => health.0 = new_health,
    }

    match new_damage == 0 {
        true => commands.entity(event).despawn_recursive(),
        false => attack.damage = new_damage,
    }
}

fn damage_random_armor_or_player(
    mut commands: Commands,
    armor: Query<Entity, With<Armor>>,
    player: Query<Entity, With<Player>>,
    key: Res<ButtonInput<KeyCode>>,
) {
    if key.just_released(KeyCode::KeyR) {
        commands.send_event(Attack { damage: 69 });
    }

    if key.just_pressed(KeyCode::Space) {
        error!("yeet");
        let mut rng = thread_rng();
        let target = armor
            .iter()
            .choose(&mut rng)
            .unwrap_or_else(|| player.single());

        commands.entity(target).send_event(Attack { damage: 3 });
    }
}
