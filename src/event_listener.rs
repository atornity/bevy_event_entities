use std::marker::PhantomData;

use bevy_app::{Plugin, PreUpdate};
use bevy_ecs::{
    all_tuples,
    entity::Entities,
    prelude::*,
    query::{QueryData, QueryFilter, QueryItem, ROQueryItem},
    schedule::{IntoSystemConfigs, ScheduleLabel, SystemConfigs},
    system::{BoxedSystem, CommandQueue, EntityCommands, IntoSystem, SystemId, SystemParam},
    world::World,
};
use bevy_hierarchy::{BuildWorldChildren, Parent};
use bevy_log::trace;
use bevy_reflect::Reflect;
use bevy_utils::intern::Interned;

use crate::{any_events, EventEntities, EventEntityReader, QueryEventReader, SendEventExt};

pub use bevy_ecs::world::EntityRef;
pub use bevy_event_entities_derive::Listenable;

// TODO: when hooks arrive, automatically run callbacks when a `Listenable` is added to an entity
pub trait Listenable: Send + Sync + 'static {
    fn entity_contains(entity: EntityRef) -> bool;
}

macro_rules! impl_listenable_tuple {
    ($($T:ident),*) => {
        impl<$($T: Listenable),*> Listenable for ($($T,)*) {
            fn entity_contains(entity: EntityRef) -> bool {
                $($T::entity_contains(entity))&&*
            }
        }
    };
}

all_tuples!(impl_listenable_tuple, 1, 4, T);

#[derive(SystemSet, PartialEq, Eq, Hash, Debug, Clone)]
pub struct EventListenerSystems;

pub fn event_listener_system_configs() -> SystemConfigs {
    IntoSystemConfigs::into_configs(
        (propagate_events, run_callbacks)
            .chain()
            .run_if(any_events)
            .in_set(EventListenerSystems),
    )
}

pub struct EventListenerPlugin {
    schedule: Interned<dyn ScheduleLabel>,
}

impl Plugin for EventListenerPlugin {
    fn build(&self, app: &mut bevy_app::App) {
        app.add_systems(EventListenerSchedule, event_listener_system_configs());
        app.add_systems(
            self.schedule.clone(),
            run_event_listener_schedule.in_set(EventListenerSystems),
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

pub fn run_event_listener_schedule(world: &mut World) {
    world.run_schedule(EventListenerSchedule);
}

#[derive(Component, Clone, PartialEq)]
pub struct PropagatedEvent(pub Entity);

pub fn propagate_events(
    mut commands: Commands,
    mut events: QueryEventReader<(Entity, &Target)>,
    query: Query<&Parent>,
) {
    for (event, &Target(mut target)) in events.read() {
        while let Ok(parent) = query.get(target) {
            target = parent.get();
            trace!("propagating event {event:?} to target {target:?}");
            commands.send_event((Target(target), PropagatedEvent(event)));
        }
    }
}

pub fn run_callbacks(world: &mut World, mut reader: Local<EventEntityReader>) {
    world.resource_scope::<EventEntities, _>(|world: &mut World, events| {
        world.insert_resource(ListenerInput { event_type: EventType::PLACEHOLDER });
        let mut queue = CommandQueue::default();
        for event in reader.read(&events) {
            let Some(target) = world.get_entity(event).map(|e| e.get::<Target>().map(|t| t.0)) else {
                continue;
            };

            let event = match world.get::<PropagatedEvent>(event) {
                Some(p) => EventType::Propagated { event: p.0, propagated: event },
                None => EventType::Event(event),
            };

            if !world.entities().contains(event.id()) {
                continue;
            }

            let mut query = world.query::<(Entity, &CallbackIdent, Option<&Parent>)>();
            for (callback_entity, ident, parent) in query.iter(world) {
                if target.and_then(|t| parent.map(|p| p.get() == t)).unwrap_or(true)
                    && ident.entity_contains(world.entity(event.id()))
                    && world.entities().contains(callback_entity)
                    && event.entities_contains(world.entities())
                    && parent.map(|p| world.entities().contains(p.get())).unwrap_or(true)
                {
                    trace!("running callback {callback_entity:?} for event {event:?} with target {target:?}");
                    queue.push(move |world: &mut World| {
                        if !event.entities_contains(world.entities()) {
                            trace!("event {:?} no longer exists", event.id());
                            return;
                        }

                        // set the input for the callback
                        let mut input = world.resource_mut::<ListenerInput>();

                        input.event_type = event;

                        // take the callback from the entity temporarily to run it
                        let Some(mut callback) = world
                            .get_entity_mut(callback_entity)
                            .and_then(|mut c| c.take::<CallbackSystem>())
                        else {
                            return;
                        };

                        // replace the target of the propagated event with the target of the actual event.
                        event.swap_target(world);

                        // run the callback
                        callback.run(world);

                        // restore the target to the previous value
                        event.swap_target(world);

                        // put the callback back into the entity if it still exists
                        if let Some(mut e) = world.get_entity_mut(callback_entity) {
                            e.insert(callback);
                        }
                    });
                }
            }
        }
        queue.apply(world);
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

#[derive(Component, Reflect, Debug, PartialEq, Clone, Copy)]
/// Add this to an event to make it listenable.
pub struct Target(pub Entity);

impl Listenable for Target {
    fn entity_contains(entity: EntityRef) -> bool {
        entity.contains::<Target>()
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum EventType {
    Propagated { propagated: Entity, event: Entity },
    Event(Entity),
}

impl EventType {
    const PLACEHOLDER: Self = EventType::Event(Entity::PLACEHOLDER);

    #[inline]
    pub fn is_propagated(&self) -> bool {
        matches!(self, EventType::Propagated { .. })
    }

    #[inline]
    pub fn id(&self) -> Entity {
        match self {
            EventType::Propagated { event, .. } => *event,
            EventType::Event(event) => *event,
        }
    }

    fn entities_contains(&self, entities: &Entities) -> bool {
        match self {
            EventType::Propagated {
                propagated: entity,
                event,
            } => entities.contains(*entity) && entities.contains(*event),
            EventType::Event(event) => entities.contains(*event),
        }
    }

    fn swap_target(&self, world: &mut World) -> bool {
        match self {
            EventType::Propagated {
                propagated: entity,
                event,
            } => {
                if !self.entities_contains(world.entities()) {
                    return false;
                }
                assert_ne!(*entity, *event);
                let cell = world.as_unsafe_world_cell();

                // Safety: the entities are not the same
                let swap = || unsafe {
                    let mut src = cell.get_entity(*entity)?.get_mut::<Target>()?;
                    let mut dst = cell.get_entity(*event)?.get_mut::<Target>()?;
                    std::mem::swap(&mut src.0, &mut dst.0);
                    Some(())
                };
                swap().is_some()
            }
            EventType::Event(_) => false,
        }
    }
}

#[derive(Resource, Debug, PartialEq, Clone)]
pub struct ListenerInput {
    pub event_type: EventType,
}

#[derive(SystemParam, Debug)]
pub struct Listener<'w, 's, D = (), F = ()>
where
    D: QueryData + 'static,
    F: QueryFilter + 'static,
{
    input: Res<'w, ListenerInput>,
    query: Query<'w, 's, D, F>,
}

impl<'w, 's, D: QueryData, F: QueryFilter> Listener<'w, 's, D, F> {
    #[inline]
    pub fn event_type(&self) -> EventType {
        self.input.event_type
    }

    #[inline]
    /// Returns true if the event is propagated. Ie. it is not the root event.
    pub fn is_propagated(&self) -> bool {
        self.event_type().is_propagated()
    }

    #[inline]
    /// Retrieve an immutable reference to the event data from the query.
    ///
    /// # Panics
    ///
    /// Will panic if the entity is not in the query.
    pub fn event(&self) -> ROQueryItem<'_, D> {
        self.get_event().unwrap()
    }

    #[inline]
    /// Retrieve a mutable reference to the event data from the query.
    ///
    /// # Panics
    ///
    /// Will panic if the entity is not in the query.
    pub fn event_mut(&mut self) -> QueryItem<'_, D> {
        self.get_event_mut().unwrap()
    }

    #[inline]
    /// Retrieve an immutable reference to the event data from the query.
    pub fn get_event(&self) -> Result<ROQueryItem<'_, D>, bevy_ecs::query::QueryEntityError> {
        self.query.get(self.input.event_type.id())
    }

    #[inline]
    /// Retrieve a mutable reference to the event data from the query.
    pub fn get_event_mut(&mut self) -> Result<QueryItem<'_, D>, bevy_ecs::query::QueryEntityError> {
        self.query.get_mut(self.input.event_type.id())
    }

    #[inline]
    /// Returns the entity of the event or the Propagated(entity) if the event is propagated
    pub fn id(&self) -> Entity {
        self.input.event_type.id()
    }

    #[inline]
    pub fn query(&self) -> &Query<'w, 's, D, F> {
        &self.query
    }

    #[inline]
    pub fn query_mut(&mut self) -> &mut Query<'w, 's, D, F> {
        &mut self.query
    }
}

#[derive(Component)]
pub struct CallbackIdent {
    fn_entity_contains: fn(EntityRef) -> bool,
}

impl CallbackIdent {
    pub fn new<T: Listenable>() -> Self {
        Self {
            fn_entity_contains: |entity| T::entity_contains(entity),
        }
    }

    pub fn entity_contains(&self, entity: EntityRef) -> bool {
        (self.fn_entity_contains)(entity)
    }
}

#[derive(Component)]
pub enum CallbackSystem {
    Pending(Option<BoxedSystem>),
    Ready(SystemId),
}

impl CallbackSystem {
    pub fn new<M>(system: impl IntoSystem<(), (), M>) -> Self {
        Self::Pending(Some(Box::new(IntoSystem::into_system(system))))
    }

    pub fn run(&mut self, world: &mut World) {
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
pub struct On<T: Listenable> {
    ident: CallbackIdent,
    system: CallbackSystem,
    marker: PhantomData<T>,
}

impl<T: Listenable> On<T> {
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

pub trait IntoCallback<T, M>: Send + Sync + 'static {
    fn into_bundle(self) -> (CallbackIdent, CallbackSystem);
}

impl<T: Listenable> IntoCallback<T, ()> for On<T> {
    #[inline]
    fn into_bundle(self) -> (CallbackIdent, CallbackSystem) {
        self.into_bundle()
    }
}

impl<T: Listenable> IntoCallback<T, ()> for CallbackSystem {
    #[inline]
    fn into_bundle(self) -> (CallbackIdent, CallbackSystem) {
        (CallbackIdent::new::<T>(), self)
    }
}

impl<M, T: Listenable, S: IntoSystem<(), (), M> + Send + Sync + 'static> IntoCallback<T, M> for S {
    #[inline]
    fn into_bundle(self) -> (CallbackIdent, CallbackSystem) {
        (CallbackIdent::new::<T>(), CallbackSystem::new(self))
    }
}

pub trait AddCallbackExt {
    /// Run a system when the event matching `T` is triggered.
    fn add_callback<T: Listenable, M>(&mut self, callback: impl IntoCallback<T, M>) -> &mut Self;
}

pub trait AddEntityCallbackExt: AddCallbackExt {
    /// Run a system when the event matching `T` is triggered with this entity as the [`Target`].
    ///
    /// See also [`entity_callback`](`AddEntityCallbackExt::entity_callback`).
    fn entity_callback<T: Listenable, M>(
        &mut self,
        callback: impl IntoCallback<T, M>,
    ) -> &mut Self {
        self.add_callback::<(Target, T), _>(callback.into_bundle().1);
        self
    }
}

impl AddCallbackExt for World {
    fn add_callback<T: Listenable, M>(&mut self, callback: impl IntoCallback<T, M>) -> &mut Self {
        self.spawn(callback.into_bundle());
        self
    }
}

impl<'w, 's> AddCallbackExt for Commands<'w, 's> {
    fn add_callback<T: Listenable, M>(&mut self, callback: impl IntoCallback<T, M>) -> &mut Self {
        self.add(move |world: &mut World| {
            world.add_callback(callback);
        });
        self
    }
}

impl<'w> AddCallbackExt for EntityWorldMut<'w> {
    /// Run a system when the event matching `T` is triggered.
    ///
    /// This will always run the callback system regardless of whether or not this entity was the [`Target`] of the event.
    ///
    /// See also [`entity_callback`](`AddEntityCallbackExt::entity_callback`).
    fn add_callback<T: Listenable, M>(&mut self, callback: impl IntoCallback<T, M>) -> &mut Self {
        let callback = self.world_scope(|world| world.spawn(callback.into_bundle()).id());
        self.add_child(callback);
        self
    }
}

impl<'w> AddEntityCallbackExt for EntityWorldMut<'w> {}

impl<'a> AddCallbackExt for EntityCommands<'a> {
    /// Run a system when the event matching `T` is triggered.
    ///
    /// This will always run the callback system regardless of whether or not this entity was the [`Target`] of the event.
    ///
    /// See also [`entity_callback`](`AddEntityCallbackExt::entity_callback`).
    fn add_callback<T: Listenable, M>(&mut self, callback: impl IntoCallback<T, M>) -> &mut Self {
        self.add(move |mut entity: EntityWorldMut| {
            entity.add_callback(callback);
        })
    }
}

impl<'a> AddEntityCallbackExt for EntityCommands<'a> {}

#[test]
// this tests if events are propagated up the tree and if it stops when the event is despawned
fn test_propagate_events() {
    #[derive(Component)]
    struct Stop;

    #[derive(Component)]
    struct Marker;

    #[derive(Component)]
    struct TestEvent;

    impl Listenable for TestEvent {
        fn entity_contains(entity: EntityRef) -> bool {
            entity.contains::<Self>()
        }
    }

    fn callback(
        mut commands: Commands,
        input: Listener<(Entity, &Target)>,
        stop: Query<(), With<Stop>>,
    ) {
        let (event, target) = input.event();
        commands.entity(target.0).insert(Marker);
        if dbg!(stop.contains(target.0)) {
            commands.entity(event).despawn();
        }
    }

    let mut world = World::new();

    world.init_resource::<EventEntities>();
    let mut schedule = Schedule::default();
    schedule.add_systems(event_listener_system_configs());

    let mut entities = Vec::new();
    for i in 0..10 {
        let entity = world
            .spawn_empty()
            .entity_callback::<TestEvent, _>(callback)
            .id();
        if i > 0 {
            world.entity_mut(entity).add_child(entities[i - 1]);
        }
        if i == 5 {
            world.entity_mut(entity).insert(Stop);
        }
        entities.push(entity);
    }

    crate::send_event(&mut world, (TestEvent, Target(entities[0])));
    schedule.run(&mut world);

    for (n, entity) in entities.into_iter().enumerate() {
        dbg!(n, world.entity(entity).contains::<Marker>());
        if n > 5 {
            assert!(!world.entity(entity).contains::<Marker>());
        } else {
            assert!(world.entity(entity).contains::<Marker>());
        }
    }
}
