# Experiment to implement a fast 2d tilemap renderer for Bevy

`bevy_ecs_tilemap` is great, but it has a ton of CPU overhead, because
it does "entity-based extraction"; that is, it iterates over all tile
entities in `Extract` and then uses entities in the `Render` world.

A larger tilemap can easily spend many milliseconds of CPU time every
frame in these two places, becoming very bottlenecked.

This crate is an experiment to create a tilemap rendering implementation
that largely keeps the same user API as `bevy_ecs_tilemap`, but avoids that
overhead. It is an explicit goal to be able to easily port things built on
top of `bevy_ecs_tilemap` (such as `bevy_ecs_ldtk`) to this crate, with few
or no changes required.

We plan to have a system in `PostUpdate` (where it can run in parallel with
other Bevy engine systems) to put data from tile entities into an efficient
representation, that can be quickly extracted and copied into GPU memory,
thus saving CPU time.

In the longer term, we hope that if our proof of concept here is successful,
the ideas from this crate can be upstreamed into `bevy_ecs_tilemap` (or even
better, Bevy proper).
