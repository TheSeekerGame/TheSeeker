use bevy::prelude::*;
use bevy::ecs::query::QueryFilter;

pub fn any_matching<F: QueryFilter>(
) -> impl FnMut(Query<(), F>) -> bool + Clone {
    move |query: Query<(), F>| !query.is_empty()
}
