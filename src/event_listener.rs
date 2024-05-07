use std::marker::PhantomData;

use bevy_app::Plugin;
use bevy_ecs::{
    component::ComponentId,
    prelude::*,
    query::{QueryData, QueryFilter, QueryItem, ROQueryItem},
    system::{BoxedSystem, EntityCommands, SystemParam},
    world::DeferredWorld,
};
use bevy_hierarchy::Parent;
use bevy_reflect::Reflect;

use crate::SendEventExt;

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
        let hooks = app.world_mut().register_component_hooks::<T>();
        hooks.on_add(event_listener_hook::<T>);
    }
}

fn event_listener_hook<T: Component>(mut world: DeferredWorld, event: Entity, _: ComponentId) {
    let Some(&Target(target)) = world.get::<Target>(event) else {
        return;
    };
    world.commands().add(move |world: &mut World| {
        let mut target = target;

        loop {
            if !world.entities().contains(event) {
                return;
            }

            let Some(mut on) = world.entity_mut(target).take::<On<T>>() else {
                continue;
            };

            world.insert_resource(ListenerInput { event, target });

            let new_target = world.get::<Parent>(target).map(|parent| parent.get());

            for callback in &mut on.callbacks {
                callback.run_and_apply(world);
            }

            if let Some(mut entity) = world.get_entity_mut(target) {
                entity.insert(on);
            }

            match new_target {
                Some(new_target) => {
                    target = new_target;
                }
                None => break,
            }
        }
        world.remove_resource::<ListenerInput>();
    });
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

#[derive(Resource, Debug, Clone, PartialEq, Component, Reflect)]
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
    #[track_caller]
    pub fn get(&self) -> ROQueryItem<'_, D> {
        self.query.get(self.input.event).unwrap()
    }

    #[track_caller]
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

    fn run_and_apply(&mut self, world: &mut World) {
        match self {
            CallbackSystem::Pending(system) => {
                let mut system = system.take().unwrap();
                system.initialize(world);
                system.run((), world);
                system.apply_deferred(world);
                *self = CallbackSystem::Ready(system);
            }
            CallbackSystem::Ready(system) => {
                system.run((), world);
                system.apply_deferred(world);
            }
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
