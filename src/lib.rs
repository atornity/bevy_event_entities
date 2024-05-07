use std::{
    iter::Chain,
    ops::{Deref, DerefMut},
    slice::Iter,
};

use bevy_app::prelude::*;
use bevy_ecs::{
    prelude::*,
    query::{QueryFilter, ReadOnlyQueryData},
    schedule::ScheduleLabel,
    system::{EntityCommands, SystemParam},
};
use bevy_reflect::Reflect;
use bevy_utils::intern::Interned;

pub mod event_listener;

pub mod prelude {
    pub use crate::{
        event_listener::{EventInput, On, SendEntityEventExt},
        EntityEventReader, EventPlugin, QueryEventReader, SendEventExt,
    };
}

#[derive(SystemSet, PartialEq, Eq, Hash, Clone, Debug)]
pub struct EventSystems;

pub struct EventPlugin(Interned<dyn ScheduleLabel>);

impl Default for EventPlugin {
    fn default() -> Self {
        Self(PostUpdate.intern())
    }
}

impl EventPlugin {
    pub fn new(schedule: impl ScheduleLabel) -> Self {
        Self(schedule.intern())
    }
}

impl Plugin for EventPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<EventEntities>();
        app.add_systems(self.0.clone(), update_events.in_set(EventSystems));
    }
}

// TODO: events should only update once the fixed schedule has finished at least once since the last update.
// If not then events may be missed if listened to from a system in fixed schedule.
fn update_events(world: &mut World) {
    world.resource_scope::<EventEntities, _>(|world, mut events| {
        for entity in events.update_drain() {
            if let Some(entity) = world.get_entity_mut(entity) {
                entity.despawn(); // TODO: should this be recursive?
            }
        }
    });
}

pub trait SendEventExt {
    /// Spawn an entity and push it to the `Events` resource. Returns the `EntityCommands` of the spawned event.
    fn send_event(&mut self, event: impl Bundle) -> EntityCommands;
}

impl<'w, 's> SendEventExt for Commands<'w, 's> {
    fn send_event(&mut self, event: impl Bundle) -> EntityCommands {
        let entity = self.spawn_empty().id();
        self.add(move |world: &mut World| {
            world.resource_mut::<EventEntities>().send(entity);
            world.entity_mut(entity).insert((Event, event));
        });
        self.entity(entity)
    }
}

#[derive(Component, Reflect, Debug, Clone, Copy)]
pub struct Event;

#[derive(Reflect, Debug, Default, Clone)]
pub struct EventSequence {
    events: Vec<Entity>,
    start_event_count: usize,
}

impl Deref for EventSequence {
    type Target = Vec<Entity>;

    fn deref(&self) -> &Self::Target {
        &self.events
    }
}

impl DerefMut for EventSequence {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.events
    }
}

#[derive(Resource, Reflect, Debug, Default, Clone)]
pub struct EventEntities {
    events_a: EventSequence,
    events_b: EventSequence,
    event_count: usize,
}

impl EventEntities {
    pub fn send(&mut self, event: Entity) {
        self.events_b.push(event);
        self.event_count += 1;
    }

    pub fn send_batch(&mut self, events: impl IntoIterator<Item = Entity>) {
        self.extend(events);
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.events_a.len() + self.events_b.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn oldest_event_count(&self) -> usize {
        self.events_a
            .start_event_count
            .min(self.events_b.start_event_count)
    }

    pub fn update_drain(&mut self) -> impl Iterator<Item = Entity> + '_ {
        std::mem::swap(&mut self.events_a, &mut self.events_b);
        let iter = self.events_b.events.drain(..);
        self.events_b.start_event_count = self.event_count;
        debug_assert_eq!(
            self.events_a.start_event_count + self.events_a.len(),
            self.events_b.start_event_count
        );
        iter
    }

    pub fn update(&mut self) {
        let _ = self.update_drain();
    }

    pub fn drain(&mut self) -> impl Iterator<Item = Entity> + '_ {
        self.reset_start_event_count();

        self.events_a.drain(..).chain(self.events_b.drain(..))
    }

    #[inline]
    pub fn clear(&mut self) {
        self.reset_start_event_count();
        self.events_a.clear();
        self.events_b.clear();
    }

    #[inline]
    fn reset_start_event_count(&mut self) {
        self.events_a.start_event_count = self.event_count;
        self.events_b.start_event_count = self.event_count;
    }
}

impl Extend<Entity> for EventEntities {
    fn extend<I>(&mut self, iter: I)
    where
        I: IntoIterator<Item = Entity>,
    {
        let mut event_count = self.event_count;
        let events = iter.into_iter().map(|event| {
            event_count += 1;
            event
        });
        self.events_b.extend(events);
        self.event_count = event_count;
    }
}

#[derive(Debug)]
pub struct ManualEventReader {
    last_event_count: usize,
}

impl Default for ManualEventReader {
    fn default() -> Self {
        Self {
            last_event_count: 0,
        }
    }
}

impl ManualEventReader {
    pub fn read<'a>(&'a mut self, events: &'a EventEntities) -> EntityEventIterator<'a> {
        EntityEventIterator::new(self, events)
    }

    pub fn read_with_query<'w, 's, 'a, D: ReadOnlyQueryData, F: QueryFilter>(
        &'a mut self,
        events: &'a EventEntities,
        query: &'a Query<'w, 's, D, F>,
    ) -> QueryEventIterator<'w, 's, 'a, D, F> {
        QueryEventIterator {
            inner: EntityEventIterator::new(self, events),
            query,
        }
    }

    pub fn len(&self, events: &EventEntities) -> usize {
        events
            .event_count
            .saturating_sub(self.last_event_count)
            .min(events.len())
    }
}

#[derive(SystemParam)]
pub struct QueryEventReader<'w, 's, D, F = ()>
where
    D: ReadOnlyQueryData + 'static,
    F: QueryFilter + 'static,
{
    reader: Local<'s, ManualEventReader>,
    events: Res<'w, EventEntities>,
    query: Query<'w, 's, D, F>,
}

impl<'w, 's, D, F> QueryEventReader<'w, 's, D, F>
where
    D: ReadOnlyQueryData,
    F: QueryFilter,
{
    pub fn read<'a>(&'a mut self) -> QueryEventIterator<'w, 's, 'a, D, F> {
        self.reader.read_with_query(&self.events, &self.query)
    }
}

#[derive(SystemParam)]
pub struct EntityEventReader<'w, 's> {
    reader: Local<'s, ManualEventReader>,
    events: Res<'w, EventEntities>,
}

impl<'w, 's> EntityEventReader<'w, 's> {
    pub fn read(&mut self) -> EntityEventIterator {
        self.reader.read(&self.events)
    }
}

#[derive(Debug)]
pub struct QueryEventIterator<'w, 's, 'a, D: ReadOnlyQueryData, F: QueryFilter> {
    inner: EntityEventIterator<'a>,
    query: &'a Query<'w, 's, D, F>,
}

impl<'w, 's, 'a, D: ReadOnlyQueryData, F: QueryFilter> Iterator
    for QueryEventIterator<'w, 's, 'a, D, F>
{
    type Item = D::Item<'w>;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(entity) = self.inner.next() {
            if let Ok(inner) = self.query.get_inner(entity) {
                return Some(inner);
            }
        }
        None
    }
}

#[derive(Debug)]
pub struct EntityEventIterator<'a> {
    reader: &'a mut ManualEventReader,
    chain: Chain<Iter<'a, Entity>, Iter<'a, Entity>>,
    unread: usize,
}

impl<'a> EntityEventIterator<'a> {
    pub fn new(reader: &'a mut ManualEventReader, events: &'a EventEntities) -> Self {
        let a_index = reader
            .last_event_count
            .saturating_sub(events.events_a.start_event_count);
        let b_index = reader
            .last_event_count
            .saturating_sub(events.events_b.start_event_count);
        let a = events.events_a.get(a_index..).unwrap_or_default();
        let b = events.events_b.get(b_index..).unwrap_or_default();

        let unread_count = a.len() + b.len();
        // Ensure `len` is implemented correctly
        debug_assert_eq!(unread_count, reader.len(events));
        reader.last_event_count = events.event_count - unread_count;
        // Iterate the oldest first, then the newer events
        let chain = a.iter().chain(b.iter());

        Self {
            reader,
            chain,
            unread: unread_count,
        }
    }
}

impl<'a> Iterator for EntityEventIterator<'a> {
    type Item = Entity;

    fn next(&mut self) -> Option<Self::Item> {
        match self.chain.next() {
            Some(entity) => {
                self.reader.last_event_count += 1;
                self.unread -= 1;
                Some(*entity)
            }
            None => None,
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.chain.size_hint()
    }

    fn count(self) -> usize {
        self.reader.last_event_count += self.unread;
        self.unread
    }

    fn last(self) -> Option<Self::Item>
    where
        Self: Sized,
    {
        let entity = self.chain.last()?;
        self.reader.last_event_count += self.unread;
        Some(*entity)
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        if let Some(entity) = self.chain.nth(n) {
            self.reader.last_event_count += n + 1;
            self.unread -= n + 1;
            Some(*entity)
        } else {
            self.reader.last_event_count += self.unread;
            self.unread = 0;
            None
        }
    }
}

impl<'a> ExactSizeIterator for EntityEventIterator<'a> {
    fn len(&self) -> usize {
        self.unread
    }
}
