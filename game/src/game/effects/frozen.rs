use bevy::camera::visibility::RenderLayers;

use crate::game::enemy::Enemy;
use crate::game::gentstate::Dead;
use crate::game::player::Player;
use crate::prelude::*;
use crate::game::actors::AiPaused;
use theseeker_engine::animation::SpriteAnimationBundle;
use theseeker_engine::assets::animation::SpriteAnimation;
use theseeker_engine::gent::{Gent, TransformGfxFromGent};
use theseeker_engine::physics::LinearVelocity;
use theseeker_engine::script::ScriptPlayer;

/// Marker component for the spawned frozen overlay entity.
#[derive(Component)]
pub struct FrozenOverlay;

/// Link component stored on the frozen actor pointing to its overlay entity.
#[derive(Component)]
pub struct FrozenVisual {
    pub overlay: Entity,
}

#[derive(Component)]
struct FrozenVelocity(Vec2);

/// Size variants for the frozen overlay visuals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrozenSize {
    Small,
    Large,
}

/// Component applied to actors that are frozen in place.
#[derive(Component, Debug)]
pub struct Frozen {
    pub remaining_ticks: u32,
    pub total_ticks: u32,
    pub size: FrozenSize,
    pub inflicted_by: Option<Entity>,
}

impl Frozen {
    pub fn new(duration_ticks: u32, size: FrozenSize) -> Self {
        Self {
            remaining_ticks: duration_ticks,
            total_ticks: duration_ticks,
            size,
            inflicted_by: None,
        }
    }

    /// Refresh the remaining duration without altering other metadata.
    pub fn refresh(&mut self, duration_ticks: u32) {
        self.remaining_ticks = duration_ticks;
        self.total_ticks = duration_ticks;
    }
}

// Overlay offsets for correct layering:
// - Positive offset places overlay slightly in front of enemies (above their base Z)
// - Negative offset keeps overlay behind the player (below player base Z)
const OVERLAY_OFFSET_ENEMY: f32 = 0.000_000_4;
const OVERLAY_OFFSET_PLAYER: f32 = -0.000_000_4;

const SMALL_OVERLAY_KEY: &str = "anim.player.IceBoulderSmall";
const LARGE_OVERLAY_KEY: &str = "anim.player.IceBoulderBig";
const PLAYER_IDLE_KEY: &str = "anim.player.Idle";

pub struct FrozenEffectPlugin;

impl Plugin for FrozenEffectPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            (
                clear_frozen_on_death,
                attach_frozen_visuals.after(clear_frozen_on_death),
                tick_frozen.after(attach_frozen_visuals),
            )
                .run_if(in_state(AppState::InGame))
                .after(crate::game::player::spawns::ice_nova::tick_ice_nova)
                .before(crate::game::enemy::EnemyStateSet::Sensors)
                .before(crate::game::enemy::dead),
        );
        app.add_systems(PostUpdate, cleanup_frozen_removed);
    }
}

fn attach_frozen_visuals(
    mut commands: Commands,
    added: Query<(Entity, &Frozen, &Gent), Added<Frozen>>,
    enemy_query: Query<(), With<Enemy>>,
    player_query: Query<(), With<Player>>,
    mut anim_players: Query<&mut ScriptPlayer<SpriteAnimation>>,
    mut velocity_query: Query<&mut LinearVelocity>,
) {
    for (entity, frozen, gent) in added.iter() {
        let is_player = player_query.get(entity).is_ok();
        let is_enemy = enemy_query.get(entity).is_ok();
        let overlay_key = match frozen.size {
            FrozenSize::Small => SMALL_OVERLAY_KEY,
            FrozenSize::Large => LARGE_OVERLAY_KEY,
        };
        let offset = if is_player {
            OVERLAY_OFFSET_PLAYER
        } else {
            OVERLAY_OFFSET_ENEMY
        };

        // Spawn overlay entity that follows the target via `TransformGfxFromGent`.
        let overlay = commands
            .spawn((
                FrozenOverlay,
                SpriteAnimationBundle::new_play_key(overlay_key),
                Sprite {
                    texture_atlas: Some(TextureAtlas::default()),
                    ..Default::default()
                },
                Transform::from_translation(Vec3::new(0.0, 0.0, offset)),
                GlobalTransform::default(),
                Visibility::Visible,
                InheritedVisibility::default(),
                ViewVisibility::default(),
                RenderLayers::layer(2),
                TransformGfxFromGent {
                    pixel_aligned: false,
                    gent: entity,
                    offset: Some(Vec3::new(0.0, 0.0, offset)),
                },
                StateDespawnMarker,
            ))
            .id();

        let saved_velocity = velocity_query.get_mut(entity).ok().map(|mut vel| {
            let saved = vel.0;
            vel.0 = Vec2::ZERO;
            saved
        });

        commands.entity(entity).insert(FrozenVisual { overlay });
        if let Some(saved_velocity) = saved_velocity {
            commands.entity(entity).insert(FrozenVelocity(saved_velocity));
        }

        update_frozen_animation(
            entity,
            gent,
            is_enemy,
            is_player,
            &mut anim_players,
            &mut commands,
        );
    }
}

fn update_frozen_animation(
    entity: Entity,
    gent: &Gent,
    is_enemy: bool,
    is_player: bool,
    anim_players: &mut Query<&mut ScriptPlayer<SpriteAnimation>>,
    commands: &mut Commands,
) {
    if is_enemy {
        commands.entity(entity).insert(AiPaused);
        set_gent_animation_paused(gent, true, anim_players);
        return;
    }
    if let (true, Ok(mut anim)) = (is_player, anim_players.get_mut(gent.e_gfx)) {
        anim.play_key(PLAYER_IDLE_KEY);
    }
}

fn tick_frozen(
    mut commands: Commands,
    mut query: Query<(Entity, &mut Frozen)>,
    mut velocity_query: Query<&mut LinearVelocity>,
) {
    for (entity, mut frozen) in query.iter_mut() {
        if let Ok(mut vel) = velocity_query.get_mut(entity) {
            vel.0 = Vec2::ZERO;
        }

        if frozen.remaining_ticks > 0 {
            frozen.remaining_ticks -= 1;
        }
        if frozen.remaining_ticks == 0 {
            commands.entity(entity).remove::<Frozen>();
        }
    }
}

fn cleanup_frozen_removed(
    mut removed: RemovedComponents<Frozen>,
    visuals: Query<(
        &FrozenVisual,
        Option<&FrozenVelocity>,
        Option<&Gent>,
        Has<Enemy>,
        Has<Dead>,
    )>,
    mut velocity_query: Query<&mut LinearVelocity>,
    mut anim_players: Query<&mut ScriptPlayer<SpriteAnimation>>,
    mut commands: Commands,
) {
    for entity in removed.read() {
        let Ok((visual, saved_velocity, gent, is_enemy, is_dead)) =
            visuals.get(entity)
        else {
            continue;
        };

        commands.entity(visual.overlay).despawn();
        commands.entity(entity).remove::<FrozenVisual>();
        commands.entity(entity).remove::<FrozenVelocity>();

        restore_frozen_velocity(
            entity,
            is_dead,
            saved_velocity,
            &mut velocity_query,
        );
        resume_frozen_enemy(
            entity,
            is_enemy,
            gent,
            &mut anim_players,
            &mut commands,
        );
    }
}

fn restore_frozen_velocity(
    entity: Entity,
    is_dead: bool,
    saved_velocity: Option<&FrozenVelocity>,
    velocity_query: &mut Query<&mut LinearVelocity>,
) {
    if is_dead {
        return;
    }
    let Some(saved_velocity) = saved_velocity else {
        return;
    };
    let Ok(mut velocity) = velocity_query.get_mut(entity) else {
        return;
    };

    velocity.0 = saved_velocity.0;
}

fn resume_frozen_enemy(
    entity: Entity,
    is_enemy: bool,
    gent: Option<&Gent>,
    anim_players: &mut Query<&mut ScriptPlayer<SpriteAnimation>>,
    commands: &mut Commands,
) {
    if !is_enemy {
        return;
    }

    commands.entity(entity).remove::<AiPaused>();
    if let Some(gent) = gent {
        set_gent_animation_paused(gent, false, anim_players);
    }
}

fn clear_frozen_on_death(
    mut commands: Commands,
    query: Query<
        (Entity, Option<&FrozenVisual>, Option<&Gent>),
        (With<Frozen>, Added<Dead>),
    >,
    mut anim_players: Query<&mut ScriptPlayer<SpriteAnimation>>,
) {
    for (entity, visual, gent) in &query {
        if let Some(visual) = visual {
            commands.entity(visual.overlay).despawn();
            commands.entity(entity).remove::<FrozenVisual>();
        }
        if let Some(gent) = gent {
            set_gent_animation_paused(gent, false, &mut anim_players);
        }
        commands.entity(entity).remove::<AiPaused>();
        commands.entity(entity).remove::<FrozenVelocity>();
        commands.entity(entity).remove::<Frozen>();
    }
}

fn set_gent_animation_paused(
    gent: &Gent,
    paused: bool,
    anim_players: &mut Query<&mut ScriptPlayer<SpriteAnimation>>,
) {
    for entity in [gent.e_gfx, gent.e_effects_gfx] {
        if let Ok(mut player) = anim_players.get_mut(entity) {
            player.set_paused(paused);
        }
    }
}
