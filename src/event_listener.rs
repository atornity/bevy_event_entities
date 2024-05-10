use std::{
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

use bevy_app::{Plugin, PreUpdate};
use bevy_ecs::{
    all_tuples,
    prelude::*,
    query::{QueryData, QueryFilter, QueryItem, ROQueryItem},
    schedule::{IntoSystemConfigs, ScheduleLabel, SystemConfigs},
    system::{BoxedSystem, EntityCommands, IntoSystem, SystemId, SystemParam},
    world::{EntityRef, World},
};
use bevy_hierarchy::{BuildWorldChildren, Parent};
use bevy_reflect::Reflect;
use bevy_utils::intern::Interned;

use crate::{any_events, QueryEventReader, SendEventExt};

pub use bevy_event_entities_derive::Listenable;
pub trait Listenable: Component + Sized {
    const PROPAGATE: bool;

    fn entity_contains(entity: EntityRef) -> bool {
        entity.contains::<Self>()
    }
}

// TODO: find a better name for this
pub trait ListanableTuple: Send + Sync + 'static {
    const PROPAGATE: bool;

    fn entity_contains(entity: bevy_ecs::world::EntityRef) -> bool;
}

impl<T: Listenable> ListanableTuple for T {
    const PROPAGATE: bool = T::PROPAGATE;

    fn entity_contains(entity: EntityRef) -> bool {
        T::entity_contains(entity)
    }
}

macro_rules! impl_event_ident_tuple {
    ($($T:ident),*) => {
        impl<$($T: ListanableTuple),*> ListanableTuple for ($($T,)*) {
            const PROPAGATE: bool = $($T::PROPAGATE)||*;

            fn entity_contains(entity: EntityRef) -> bool {
                $($T::entity_contains(entity))&&*
            }
        }
        impl<$($T: ListanableTuple),*> ListanableTuple for Or<($($T,)*)> {
            const PROPAGATE: bool = $($T::PROPAGATE)||*;

            fn entity_contains(entity: EntityRef) -> bool {
                $($T::entity_contains(entity))||*
            }
        }
    };
}

all_tuples!(impl_event_ident_tuple, 1, 4, T);

#[derive(SystemSet, PartialEq, Eq, Hash, Debug, Clone)]
pub struct EventListenerSystems;

pub fn event_listener_systems() -> SystemConfigs {
    IntoSystemConfigs::into_configs(
        (propagate_events, run_callbacks)
            .run_if(any_events)
            .in_set(EventListenerSystems)
            .chain(),
    )
}

pub struct EventListenerPlugin {
    schedule: Interned<dyn ScheduleLabel>,
}

impl Plugin for EventListenerPlugin {
    fn build(&self, app: &mut bevy_app::App) {
        app.add_systems(EventListenerSchedule, event_listener_systems());
        app.add_systems(
            self.schedule.clone(),
            EventListenerSchedule::run.in_set(EventListenerSystems),
        );
    }
}

impl Default for EventListenerPlugin {
    fn default() -> Self {
        Self {
            schedule: PreUpdate.intern(),
        }
    }
}

impl EventListenerPlugin {
    pub fn new(schedule: impl ScheduleLabel) -> Self {
        Self {
            schedule: schedule.intern(),
        }
    }
}

#[derive(ScheduleLabel, Debug, PartialEq, Eq, Hash, Clone)]
pub struct EventListenerSchedule;

impl EventListenerSchedule {
    pub fn run(world: &mut World) {
        world.run_schedule(EventListenerSchedule);
    }
}

#[derive(Component)]
struct PropagatedEvent(Entity);

fn propagate_events(
    mut commands: Commands,
    mut events: QueryEventReader<(Entity, &Target)>,
    query: Query<&Parent>,
) {
    for (event, &Target(mut target)) in events.read() {
        while let Ok(parent) = query.get(target) {
            target = parent.get();
            commands.entity(target).send_event(PropagatedEvent(event));
        }
    }
}

fn run_callbacks(
    mut commands: Commands,
    mut events: QueryEventReader<(Entity, &Target, Option<&PropagatedEvent>)>,
    query: Query<(Entity, &Parent, &CallbackIdent)>,
    entities: Query<EntityRef>,
) {
    for (mut event, &Target(target), propagated) in events.read() {
        if let Some(&PropagatedEvent(event_target)) = propagated {
            event = event_target;
        }
        let Ok(entity) = entities.get(event) else {
            continue;
        };
        for (entity, _, _) in query
            .iter()
            .filter(|(_, parent, ident)| parent.get() == target && ident.entity_contains(entity))
        {
            if !entities.contains(event) || !entities.contains(target) {
                continue;
            }
            commands.add(move |world: &mut World| {
                if !world.entities().contains(event) || !world.entities().contains(target) {
                    return;
                }
                let mut callback = world.entity_mut(entity).take::<CallbackSystem>().unwrap();
                world.insert_resource(ListenerInput { event, target });
                callback.run(world);
                world.remove_resource::<ListenerInput>();
                world.entity_mut(entity).insert(callback);
            });
        }
    }
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

#[derive(Component, Reflect, Debug, PartialEq, Clone, Copy)]
/// Add this to an event to make it listenable.
pub struct Target(pub Entity);

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
    item: QueryItem<'w, D>,
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

#[derive(SystemParam, Debug)]
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
            event: self.event(),
            target: self.target(),
        })
    }

    pub fn get_mut(&mut self) -> Result<EventInputMut<D>, bevy_ecs::query::QueryEntityError> {
        let event = self.event();
        let target = self.target();
        self.query
            .get_mut(self.input.event)
            .map(|item| EventInputMut {
                item,
                event,
                target,
            })
    }

    pub fn event(&self) -> Entity {
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

#[derive(Component)]
struct CallbackIdent {
    fn_entity_contains: fn(EntityRef) -> bool,
}

impl CallbackIdent {
    fn new<T: ListanableTuple>() -> Self {
        Self {
            fn_entity_contains: |entity| T::entity_contains(entity),
        }
    }

    fn entity_contains(&self, entity: EntityRef) -> bool {
        (self.fn_entity_contains)(entity)
    }
}

#[derive(Component)]
enum CallbackSystem {
    Pending(Option<BoxedSystem>),
    Ready(SystemId),
}

impl CallbackSystem {
    fn new<M>(system: impl IntoSystem<(), (), M>) -> Self {
        Self::Pending(Some(Box::new(IntoSystem::into_system(system))))
    }

    fn run(&mut self, world: &mut World) {
        match self {
            CallbackSystem::Pending(system) => {
                let id = world.register_boxed_system(system.take().unwrap());
                world.run_system(id).unwrap();
                *self = CallbackSystem::Ready(id);
            }
            CallbackSystem::Ready(id) => {
                world.run_system(*id).unwrap();
            }
        }
    }
}

#[derive(Component)]
pub struct On<T: ListanableTuple> {
    ident: CallbackIdent,
    system: CallbackSystem,
    marker: PhantomData<T>,
}

impl<T: ListanableTuple> On<T> {
    pub fn run<M>(system: impl IntoSystem<(), (), M>) -> Self {
        Self {
            marker: PhantomData,
            ident: CallbackIdent::new::<T>(),
            system: CallbackSystem::new(system),
        }
    }

    fn into_bundle(self) -> (CallbackIdent, CallbackSystem) {
        (self.ident, self.system)
    }
}

pub trait AddCallbackExt {
    fn add_callback<T: ListanableTuple>(&mut self, callback: On<T>) -> &mut Self;
    fn on<T: ListanableTuple, M>(&mut self, system: impl IntoSystem<(), (), M>) -> &mut Self {
        self.add_callback(On::<T>::run(system))
    }
}

impl<'w> AddCallbackExt for EntityWorldMut<'w> {
    fn add_callback<T: ListanableTuple>(&mut self, callback: On<T>) -> &mut Self {
        let callback = self.world_scope(|world| world.spawn(callback.into_bundle()).id());
        self.add_child(callback);
        self
    }
}

impl<'a> AddCallbackExt for EntityCommands<'a> {
    fn add_callback<T: ListanableTuple>(&mut self, callback: On<T>) -> &mut Self {
        self.add(move |mut entity: EntityWorldMut| {
            entity.add_callback(callback);
        })
    }
}
