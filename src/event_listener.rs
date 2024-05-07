use std::marker::PhantomData;

use bevy_app::{Plugin, PostUpdate};
use bevy_ecs::{
    bundle::Bundle,
    component::Component,
    entity::Entity,
    query::{QueryData, QueryFilter, QueryItem, ROQueryItem, With},
    schedule::{IntoSystemConfigs, SystemSet},
    system::{
        BoxedSystem, Commands, EntityCommands, IntoSystem, Query, Res, Resource, SystemParam,
    },
    world::World,
};
use bevy_hierarchy::Parent;
use bevy_reflect::Reflect;

use crate::{QueryEventReader, SendEventExt};

pub trait SendEntityEventExt {
    /// Same as `Commands::send_event((Target(..), ..))` except this returns `&mut Self` instead of the `EntityCommands` of the spawned event.
    fn send_event(&mut self, event: impl Bundle) -> &mut Self;
}

impl<'a> SendEntityEventExt for EntityCommands<'a> {
    fn send_event(&mut self, event: impl Bundle) -> &mut Self {
        let target = self.id();
        self.commands().send_event((Target(target), event));
        self
    }
}

#[derive(SystemSet, PartialEq, Eq, Hash, Debug, Clone)]
pub struct EventListenerSystems;

pub struct EventListenerPlugin<T: Component>(PhantomData<T>);

impl<T: Component> Plugin for EventListenerPlugin<T> {
    fn build(&self, app: &mut bevy_app::App) {
        app.add_systems(
            PostUpdate,
            (propagate_events::<T>, run_callbacks::<T>)
                .chain()
                .in_set(EventListenerSystems),
        );
    }
}

impl<T: Component> Default for EventListenerPlugin<T> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

#[derive(Component, Reflect, Debug, PartialEq, Clone)]
/// Add this to an event to make it listenable.
pub struct Target(pub Entity);

/// Useful for things like attacks etc.
#[derive(Component, Reflect, Debug, PartialEq, Clone)]
pub struct Instigator(pub Entity);

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
    pub fn get(&self) -> ROQueryItem<'_, D> {
        self.query.get(self.input.event).unwrap()
    }

    pub fn get_mut(&mut self) -> QueryItem<'_, D> {
        self.query.get_mut(self.input.event).unwrap()
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
struct Propagated<T: Component> {
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

fn propagate_events<T: Component>(
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

fn run_callbacks<T: Component>(
    mut commands: Commands,
    mut events: QueryEventReader<(Entity, &Target, Option<&Propagated<T>>)>,
    query: Query<(), With<On<T>>>,
) {
    for (entity, &Target(target), propagated) in events.read() {
        let event = propagated.map(|p| p.event).unwrap_or(entity);

        if query.contains(target) {
            commands.add(move |world: &mut World| {
                if world.get_entity(event).is_none() {
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
