use std::{
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

use bevy_app::prelude::*;
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;

use crate::SendEventExt;

pub struct CaptureEventPlugin<T: Event + Clone> {
    marker: PhantomData<T>,
}

impl<T: Event + Clone> Plugin for CaptureEventPlugin<T> {
    fn build(&self, app: &mut App) {
        app.add_systems(PreUpdate, capture_event::<T>);
    }
}

impl<T: Event + Clone> Default for CaptureEventPlugin<T> {
    fn default() -> Self {
        Self {
            marker: PhantomData,
        }
    }
}

#[derive(Component, Reflect, PartialEq, Clone)]
pub struct Captured<T>(T); // TODO: rename this

impl<T> Deref for Captured<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for Captured<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

fn capture_event<T: Event + Clone>(mut commands: Commands, mut events: EventReader<T>) {
    for event in events.read() {
        commands.send_event(Captured(event.clone()));
    }
}
