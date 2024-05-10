// bevy_skade, bevy_bane

use bevy_app::prelude::*;
use bevy_ecs::{entity::Entities, prelude::*, schedule::ScheduleLabel};
use bevy_hierarchy::DespawnRecursiveExt;

use crate::{event_listener::event_listener_systems, events_not_empty, prelude::*};

#[derive(SystemSet, PartialEq, Eq, Clone, Copy, Hash, Debug)]
pub struct DamageSystems;

pub struct DamagePlugin;

impl Plugin for DamagePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            DamageSchedule::Attack,
            (event_listener_systems(), deal_damage).chain(),
        );
        app.add_systems(
            DamageSchedule::Kill,
            (event_listener_systems(), kill_entities).chain(),
        );
        app.add_systems(
            PreUpdate,
            run_damage_schedule
                .run_if(events_not_empty)
                .in_set(DamageSystems),
        );
    }
}

#[derive(ScheduleLabel, PartialEq, Eq, Clone, Copy, Hash, Debug)]
pub enum DamageSchedule {
    Attack,
    Kill,
}

pub fn run_damage_schedule(world: &mut World) {
    loop {
        let event_count = world.resource::<EventEntities>().len();
        world.run_schedule(DamageSchedule::Attack);
        world.run_schedule(DamageSchedule::Kill);
        if world.resource::<EventEntities>().len() == event_count {
            break;
        }
    }
}

pub fn deal_damage(
    mut commands: Commands,
    mut events: QueryEventReader<(Entity, &Damage, &Target, Option<&Instigator>)>,
    mut query: Query<&mut Health>,
) {
    for (attack, &Damage(damage), &Target(target), instigator) in events.read() {
        let Ok(mut health) = query.get_mut(target) else {
            continue;
        };
        let new_health = health.value.saturating_sub(damage);
        health.value = new_health;
        if new_health == 0 {
            let mut event = commands.send_event((Target(target), Kill::new(attack)));
            if let Some(instigator) = instigator {
                event.insert(*instigator);
            }
        }
    }
}

pub fn kill_entities(
    mut commands: Commands,
    mut events: QueryEventReader<(&Kill, &Target, Option<&Instigator>)>,
    entities: &Entities,
) {
    for (&Kill { attack }, &Target(target), instigator) in events.read() {
        if entities.contains(target) {
            commands.entity(target).despawn_recursive();
        }
    }
}

#[derive(Component)]
pub struct Damage(pub u32);

#[derive(Component, Default)]
pub struct Kill {
    pub attack: Option<Entity>,
}

impl Kill {
    pub fn new(attack: Entity) -> Self {
        Self {
            attack: Some(attack),
        }
    }
}

#[derive(Component)]
pub struct Health {
    pub value: u32,
    pub max: u32,
}

impl Health {
    pub fn new(health: u32) -> Self {
        Self {
            value: health,
            max: health,
        }
    }

    #[inline(always)]
    pub fn reset(&mut self) {
        self.value = self.max;
    }

    #[inline(always)]
    pub fn shift(&mut self, val: i64) {
        self.value = (self.value as i64).saturating_add(val).min(self.max as i64) as u32
    }

    #[inline(always)]
    pub fn add(&mut self, val: u32) {
        self.value = self.value.saturating_add(val).min(self.max)
    }

    #[inline(always)]
    pub fn sub(&mut self, val: u32) {
        self.value = self.value.saturating_sub(val)
    }
}
