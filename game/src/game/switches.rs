use rapier2d::prelude::InteractionGroups;
use theseeker_engine::animation::SpriteAnimationBundle;
use theseeker_engine::assets::animation::SpriteAnimation;
use theseeker_engine::gent::{Gent, TransformGfxFromGent};
use theseeker_engine::physics::{Collider, PhysicsWorld, PLAYER, SENSOR};
use theseeker_engine::script::ScriptPlayer;

use crate::prelude::*;

use super::player::{Idle, Player};

pub struct SwitchesPlugin;

impl Plugin for SwitchesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            (
                setup_switches.run_if(any_matching::<Added<Switch>>()),
                setup_puzzles.run_if(any_matching::<Added<Switch>>()),
                activate_switches.run_if(any_matching::<(
                    With<Player>,
                    Without<Idle>,
                )>()),
            )
                .run_if(in_state(AppState::InGame)),
        );
    }
}

#[derive(Component, Default)]
struct Switch;

#[derive(Bundle, LdtkEntity, Default)]
pub struct SwitchBundle {
    marker: Switch,
}

#[derive(Component)]
struct SwitchGfx {
    pub e_gent: Entity,
}

#[derive(Bundle)]
struct SwitchGfxBundle {
    marker: SwitchGfx,
    gent2gfx: TransformGfxFromGent,
    sprite: SpriteSheetBundle,
    animation: SpriteAnimationBundle,
}

#[derive(Component, Default)]
struct Puzzle;

#[derive(Component, Default, PartialEq)]
struct PuzzleId(u8);

#[derive(Bundle, LdtkEntity, Default)]
pub struct PuzzleBundle {
    marker: Puzzle,
}

#[derive(Component)]
struct PuzzleGfx {
    pub e_gent: Entity,
}

#[derive(Bundle)]
struct PuzzleGfxBundle {
    marker: PuzzleGfx,
    gent2gfx: TransformGfxFromGent,
    sprite: SpriteSheetBundle,
    animation: SpriteAnimationBundle,
}

fn setup_switches(
    mut q: Query<(&mut Transform, Entity, &Name), Added<Switch>>,
    mut commands: Commands,
) {
    for (mut xf_gent, e_gent, name) in q.iter_mut() {
        // Manual correction of the Switch sprite positioning
        xf_gent.translation.y -= 6.0;

        let mut player = ScriptPlayer::<SpriteAnimation>::default();
        let id = parse_puzzle_id(name, "Switch");
        let e_gfx = commands.spawn_empty().id();
        let e_effects_gfx = commands.spawn_empty().id();

        player.play_key("anim.switch.flip");
        commands
            .entity(e_gent)
            .insert((
                PuzzleId(id),
                Gent {
                    e_gfx,
                    e_effects_gfx,
                },
                Collider::cuboid(
                    30.0,
                    30.0,
                    InteractionGroups {
                        memberships: SENSOR,
                        filter: PLAYER,
                    },
                ),
            ))
            .remove_parent();
        commands.entity(e_gfx).insert((
            SwitchGfxBundle {
                marker: SwitchGfx { e_gent },
                gent2gfx: TransformGfxFromGent {
                    pixel_aligned: false,
                    gent: e_gent,
                },
                sprite: SpriteSheetBundle {
                    transform: *xf_gent,
                    ..Default::default()
                },
                animation: SpriteAnimationBundle { player },
            },
            StateDespawnMarker,
        ));
    }
}

fn setup_puzzles(
    mut q: Query<(&mut Transform, Entity, &Name), Added<Puzzle>>,
    mut commands: Commands,
) {
    for (mut xf_gent, e_gent, name) in q.iter_mut() {
        let mut player = ScriptPlayer::<SpriteAnimation>::default();
        let id = parse_puzzle_id(name, "Puzzle");
        let e_gfx = commands.spawn_empty().id();

        player.play_key(format!("anim.puzzle.{:0>2}", id).as_str());
        commands.entity(e_gent).insert(PuzzleId(id));
        commands.entity(e_gfx).insert((
            PuzzleGfxBundle {
                marker: PuzzleGfx { e_gent },
                gent2gfx: TransformGfxFromGent {
                    pixel_aligned: false,
                    gent: e_gent,
                },
                sprite: SpriteSheetBundle {
                    transform: *xf_gent,
                    visibility: Visibility::Hidden,
                    ..Default::default()
                },
                animation: SpriteAnimationBundle { player },
            },
            StateDespawnMarker,
        ));
    }
}

fn activate_switches(
    query: Query<
        (
            Entity,
            &Gent,
            &GlobalTransform,
            &Collider,
        ),
        With<Switch>,
    >,
    puzzle_id_query: Query<&PuzzleId>,
    spatial_query: Res<PhysicsWorld>,
    mut puzzle_visibility_query: Query<(&PuzzleGfx, &mut Visibility)>,
    mut switch_animation_query: Query<
        &mut ScriptPlayer<SpriteAnimation>,
        With<SwitchGfx>,
    >,
) {
    for (entity, gent, transform, collider) in query.iter() {
        let intersections = spatial_query.intersect(
            transform.translation().xy(),
            collider.0.shape(),
            InteractionGroups {
                memberships: SENSOR,
                filter: PLAYER,
            },
            Some(entity),
        );

        if let Ok(mut animation) = switch_animation_query.get_mut(gent.e_gfx) {
            let should_activate_switch = !intersections.is_empty();
            animation.set_slot("Activated", should_activate_switch);

            if let Ok(switch_puzzle_id) = puzzle_id_query.get(entity) {
                if let Some((_, mut visibility)) = puzzle_visibility_query
                    .iter_mut()
                    .find(|(puzzle_gfx, _)| {
                        puzzle_id_query
                            .get(puzzle_gfx.e_gent)
                            .is_ok_and(|id| id == switch_puzzle_id)
                    })
                {
                    *visibility = if should_activate_switch {
                        Visibility::Visible
                    } else {
                        Visibility::Hidden
                    };
                }
            }
        }
    }
}

/// Attempts to extract a numeric ID from a [Name] component by stripping a [str] prefix.
///
/// Returns 0 if the parsing is unsuccessful.
fn parse_puzzle_id(name: &Name, prefix: &str) -> u8 {
    name.strip_prefix(prefix)
        .and_then(|id| id.parse::<u8>().ok())
        .unwrap_or_default()
}
