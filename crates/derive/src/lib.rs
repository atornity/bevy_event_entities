use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput};

#[proc_macro_derive(Listenable)]
pub fn derive_listener(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let expanded = quote! {
        impl #impl_generics bevy_event_entities::event_listener::Listenable for #name #ty_generics #where_clause {
            fn entity_contains(entity: bevy_event_entities::derive_exports::EntityRef) -> bool {
                entity.contains::<Self>()
            }
        }
    };

    TokenStream::from(expanded)
}
