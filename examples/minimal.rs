use std::time::Duration;

use bevy::{prelude::*, time::common_conditions::on_timer};

use bevy_events_as_entities::prelude::*;

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, EventPlugin::default()))
        .add_systems(Update, snd)
        .add_systems(FixedUpdate, rcv)
        .run()
}

#[derive(Component)]
struct Message(String);

#[derive(Component)]
struct Count(u32);

fn snd(mut count: Local<u32>, mut commands: Commands, key: Res<ButtonInput<KeyCode>>) {
    for pressed in key.get_just_pressed() {
        commands.send_event((Message(format!("Hello, {pressed:?}!")), Count(*count)));
        *count += 1;
    }
}

fn rcv(mut reader: QueryEventReader<(&Message, &Count)>) {
    for (Message(msg), Count(count)) in reader.read() {
        info!("msg: {msg}, count: {count}");
    }
}
