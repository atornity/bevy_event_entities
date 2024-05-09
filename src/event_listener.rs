use std::{
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

use bevy_app::{Plugin, PreUpdate};
use bevy_ecs::{
    bundle::Bundle,
    component::Component,
    entity::Entity,
    query::{Or, QueryData, QueryFilter, QueryItem, ROQueryItem, With},
    schedule::{IntoSystemConfigs, ScheduleLabel, SystemConfigs, SystemSet},
    system::{
        BoxedSystem, Commands, EntityCommands, IntoSystem, Query, Res, Resource, SystemParam,
    },
    world::World,
};
use bevy_hierarchy::Parent;
use bevy_log::warn;
use bevy_reflect::Reflect;
use bevy_utils::intern::Interned;

use crate::{QueryEventReader, SendEventExt};

#[derive(SystemSet, PartialEq, Eq, Hash, Debug, Clone)]
pub struct EventListenerSystems;

pub struct EventListenerPlugin<T: Component> {
    schedule: Interned<dyn ScheduleLabel>,
    marker: PhantomData<T>,
}

pub fn event_listener_systems<T: Component>() -> SystemConfigs {
    IntoSystemConfigs::into_configs(
        (propagate_events::<T>, run_callbacks::<T>)
            .in_set(EventListenerSystems)
            .chain(),
    )
}

impl<T: Component> Plugin for EventListenerPlugin<T> {
    fn build(&self, app: &mut bevy_app::App) {
        app.add_systems(self.schedule.clone(), event_listener_systems::<T>());
    }
}

impl<T: Component> Default for EventListenerPlugin<T> {
    fn default() -> Self {
        Self {
            schedule: PreUpdate.intern(),
            marker: PhantomData,
        }
    }
}

impl<T: Component> EventListenerPlugin<T> {
    pub fn new(schedule: impl ScheduleLabel) -> Self {
        Self {
            schedule: schedule.intern(),
            marker: PhantomData,
        }
    }
}

pub fn propagate_events<T: Component>(
    mut commands: Commands,
    mut events: QueryEventReader<(Entity, &Target), With<T>>,
    parents: Query<&Parent>,
) {
    for (event, &Target(mut target)) in events.read() {
        while let Ok(parent) = parents.get(target) {
            target = parent.get();
            commands
                .entity(target)
                .send_event(Propagated::<T>::new(event));
        }
    }
}

pub fn run_callbacks<T: Component>(
    mut commands: Commands,
    mut events: QueryEventReader<
        (Entity, &Target, Option<&Propagated<T>>),
        Or<(With<T>, With<Propagated<T>>)>,
    >,
    callbacks: Query<(), With<On<T>>>,
) {
    for (entity, &Target(target), propagated) in events.read() {
        let event = propagated.map(|p| p.event).unwrap_or(entity);

        if callbacks.contains(target) {
            commands.add(move |world: &mut World| {
                let entities = world.entities();
                if !entities.contains(event) {
                    warn!("event {event:?} does not exist");
                    return;
                }
                if !entities.contains(target) {
                    warn!("target {target:?} does not exist");
                    return;
                }
                world.insert_resource(ListenerInput { event, target });
                let mut on = world.entity_mut(target).take::<On<T>>().unwrap();
                for callback in &mut on.callbacks {
                    callback.run(world);
                    callback.apply_deferred(world);
                }
                if let Some(mut entity) = world.get_entity_mut(target) {
                    entity.insert(on);
                }
            });
        }
    }
    commands.add(|world: &mut World| {
        world.remove_resource::<ListenerInput>();
    })
}

pub trait SendEntityEventExt {
    fn send_event(&mut self, event: impl Bundle) -> &mut Self;
}

impl<'a> SendEntityEventExt for EntityCommands<'a> {
    fn send_event(&mut self, event: impl Bundle) -> &mut Self {
        let target = self.id();
        self.commands().send_event((Target(target), event));
        self
    }
}

#[derive(Component, Reflect, Debug, PartialEq, Clone)]
/// Add this to an event to make it listenable.
pub struct Target(pub Entity);

/// Useful for things like attacks etc.
#[derive(Component, Reflect, Debug, PartialEq, Clone)]
pub struct Instigator(pub Entity);

pub struct EventInputRef<'w, D: QueryData> {
    pub item: ROQueryItem<'w, D>,
    pub event: Entity,
    pub target: Entity,
}

impl<'w, D: QueryData> Deref for EventInputRef<'w, D> {
    type Target = ROQueryItem<'w, D>;

    fn deref(&self) -> &Self::Target {
        &self.item
    }
}

pub struct EventInputMut<'w, D: QueryData> {
    pub item: QueryItem<'w, D>,
    pub event: Entity,
    pub target: Entity,
}

impl<'w, D: QueryData> Deref for EventInputMut<'w, D> {
    type Target = QueryItem<'w, D>;

    fn deref(&self) -> &Self::Target {
        &self.item
    }
}

impl<'w, D: QueryData> DerefMut for EventInputMut<'w, D> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.item
    }
}

#[derive(Resource, Debug, PartialEq, Clone)]
pub struct ListenerInput {
    pub event: Entity,
    pub target: Entity,
}

#[derive(SystemParam)]
pub struct EventInput<'w, 's, D = (), F = ()>
where
    D: QueryData + 'static,
    F: QueryFilter + 'static,
{
    input: Res<'w, ListenerInput>,
    query: Query<'w, 's, D, F>,
}

impl<'w, 's, D: QueryData, F: QueryFilter> EventInput<'w, 's, D, F> {
    pub fn get(&self) -> Result<EventInputRef<D>, bevy_ecs::query::QueryEntityError> {
        self.query.get(self.input.event).map(|item| EventInputRef {
            item,
            event: self.id(),
            target: self.target(),
        })
    }

    pub fn get_mut(&mut self) -> Result<EventInputMut<D>, bevy_ecs::query::QueryEntityError> {
        let event = self.id();
        let target = self.target();
        self.query
            .get_mut(self.input.event)
            .map(|item| EventInputMut {
                item,
                event,
                target,
            })
    }

    pub fn id(&self) -> Entity {
        self.input.event
    }

    pub fn target(&self) -> Entity {
        self.input.target
    }

    pub fn query(&self) -> &Query<'w, 's, D, F> {
        &self.query
    }

    pub fn query_mut(&mut self) -> &mut Query<'w, 's, D, F> {
        &mut self.query
    }
}

enum CallbackSystem {
    Pending(Option<BoxedSystem>),
    Ready(BoxedSystem),
}

impl CallbackSystem {
    fn new<M>(system: impl IntoSystem<(), (), M>) -> Self {
        Self::Pending(Some(Box::new(IntoSystem::into_system(system))))
    }

    fn run(&mut self, world: &mut World) {
        match self {
            CallbackSystem::Pending(system) => {
                let mut system = system.take().unwrap();
                system.initialize(world);
                system.run((), world);
                *self = CallbackSystem::Ready(system);
            }
            CallbackSystem::Ready(system) => {
                system.run((), world);
            }
        }
    }

    fn apply_deferred(&mut self, world: &mut World) {
        match self {
            CallbackSystem::Ready(system) => {
                system.apply_deferred(world);
            }
            CallbackSystem::Pending(_) => {}
        }
    }
}

#[derive(Component)]
pub struct On<T: Component> {
    callbacks: Vec<CallbackSystem>,
    marker: PhantomData<T>,
}

impl<T: Component> On<T> {
    pub fn run<M>(system: impl IntoSystem<(), (), M>) -> Self {
        Self {
            callbacks: vec![CallbackSystem::new(system)],
            marker: PhantomData,
        }
    }

    pub fn then_run<M>(mut self, system: impl IntoSystem<(), (), M>) -> Self {
        self.callbacks.push(CallbackSystem::new(system));
        self
    }
}

#[derive(Component, Reflect, Debug)]
pub struct Propagated<T: Component> {
    event: Entity,
    marker: PhantomData<T>,
}

impl<T: Component> Clone for Propagated<T> {
    fn clone(&self) -> Self {
        Self {
            event: self.event,
            marker: PhantomData,
        }
    }
}

impl<T: Component> Propagated<T> {
    fn new(event: Entity) -> Self {
        Self {
            event,
            marker: PhantomData,
        }
    }
}
