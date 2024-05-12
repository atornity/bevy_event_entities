pub use bevy_event_entities_core::*;

pub mod event_listener {
    pub use bevy_event_entities_derive::Listenable;
    pub use bevy_event_entities_listener::*;
}

pub mod prelude {
    pub use bevy_event_entities_core::prelude::*;
    pub use bevy_event_entities_derive::*;
    pub use bevy_event_entities_listener::prelude::*;
}

pub mod derive_exports {
    pub use bevy_ecs::world::EntityRef; // required for `#[derive(Listenable)]`
}
