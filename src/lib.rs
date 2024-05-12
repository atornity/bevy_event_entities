pub use bevy_event_entities_core::*;

#[cfg(feature = "event_listener")]
pub mod event_listener {
    pub use bevy_event_entities_derive::Listenable;
    pub use bevy_event_entities_listener::*;
}

pub mod prelude {
    pub use bevy_event_entities_core::prelude::*;
    #[cfg(feature = "derive")]
    pub use bevy_event_entities_derive::*;
    #[cfg(feature = "event_listener")]
    pub use bevy_event_entities_listener::prelude::*;
}

#[cfg(feature = "derive")]
pub mod derive_exports {
    pub use bevy_ecs::world::EntityRef; // required for `#[derive(Listenable)]`
}
