use bevy::prelude::*;

pub fn any_with_components<T: Component, N: Component>(
) -> impl FnMut(Query<(), (With<T>, With<N>)>) -> bool + Clone {
    move |query: Query<(), (With<T>, With<N>)>| !query.is_empty()
}
