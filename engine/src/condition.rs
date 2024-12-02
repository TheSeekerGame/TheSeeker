use bevy::ecs::query::QueryFilter;
use bevy::prelude::*;

pub fn any_matching<F: QueryFilter>() -> impl FnMut(Query<(), F>) -> bool + Clone
{
    move |query: Query<(), F>| !query.is_empty()
}
