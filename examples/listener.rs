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
            Name::new("Player"),
            Player,
            Health(10),
            On::<Attack>::run(damage_stuff),
        ))
        .with_children(|parent| {
            parent.spawn((
                Name::new("Helmet"),
                Armor,
                Health(2),
                On::<Attack>::run(damage_stuff),
            ));
            parent.spawn((
                Name::new("Chest Plate"),
                Armor,
                Health(5),
                On::<Attack>::run(damage_stuff),
            ));
        });
}

fn damage_stuff(
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
        true => commands.entity(attack_input.id()).despawn(),
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
