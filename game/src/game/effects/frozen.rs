use bevy::prelude::*;
use bevy::render::view::RenderLayers;

use crate::game::enemy::{
    Defense, Enemy, EnemyGfx, EnemyVariant, JustThawed, MovementState,
    MovementType, Tier,
};
use crate::game::gentstate::Dead;
use crate::game::player::{Player, PlayerGfx};
use crate::prelude::*;
use theseeker_engine::ai::FsmInstance;
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
const OVERLAY_OFFSET_ENEMY: f32 = 0.0000004;
const OVERLAY_OFFSET_PLAYER: f32 = -0.0000004;

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
                .before(crate::game::enemy::dead),
        );
        app.add_systems(PostUpdate, cleanup_frozen_removed);
    }
}

fn attach_frozen_visuals(
    mut commands: Commands,
    mut added: Query<(Entity, &mut Frozen, &Gent), Added<Frozen>>,
    enemy_query: Query<(), With<Enemy>>,
    player_query: Query<(), With<Player>>,
    mut enemy_state_query: Query<&mut MovementState>,
    mut enemy_fsm_query: Query<&mut FsmInstance>,
    mut anim_players: ParamSet<(
        Query<&mut ScriptPlayer<SpriteAnimation>, With<EnemyGfx>>,
        Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>,
    )>,
    mut velocity_query: Query<&mut LinearVelocity>,
    tier_query: Query<&Tier>,
) {
    for (entity, mut frozen, gent) in added.iter_mut() {
        let is_player = player_query.get(entity).is_ok();
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
        commands.entity(entity).insert(FrozenVisual { overlay });

        // Immediately clamp velocity
        if let Ok(mut vel) = velocity_query.get_mut(entity) {
            vel.0 = Vec2::ZERO;
        }

        if enemy_query.get(entity).is_ok() {
            // Force enemies into idle state and play idle animation for visual consistency.
            let mut idle_variant = None;
            if let Ok(mut state) = enemy_state_query.get_mut(entity) {
                idle_variant = Some(state.enemy_variant);
                state.movement_type = MovementType::Idle;
                state.ticks = 0;
                state.prev_frame = None;
            }
            if let Ok(mut fsm) = enemy_fsm_query.get_mut(entity) {
                fsm.actions.clear();
                fsm.state_tick = 0;
                fsm.anim_tick = 0;
            }
            let tier = tier_query.get(entity).copied().unwrap_or(Tier::Base);
            if let Some(variant) = idle_variant {
                let idle_key = enemy_idle_animation_key(tier, variant);
                if let Ok(mut anim) = anim_players.p0().get_mut(gent.e_gfx) {
                    anim.play_key(&idle_key);
                }
            }
            // Break defensive stance while frozen
            commands.entity(entity).remove::<Defense>();
        } else if is_player {
            if let Ok(mut anim) = anim_players.p1().get_mut(gent.e_gfx) {
                anim.play_key(PLAYER_IDLE_KEY);
            }
        }
    }
}

fn enemy_idle_animation_key(tier: Tier, variant: EnemyVariant) -> String {
    let prefix = match variant {
        EnemyVariant::BigSpider => "anim.spider",
        EnemyVariant::SmallSpider => "anim.smallspider",
        EnemyVariant::Default => "anim.smallspider",
    };

    let suffix = match tier {
        Tier::Base => "",
        Tier::Two => "2",
        Tier::Three => "3",
    };

    format!("{prefix}{suffix}.Idle")
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
    mut visuals: Query<(Entity, &FrozenVisual)>,
    mut commands: Commands,
    enemy_query: Query<(), With<Enemy>>,
) {
    for entity in removed.read() {
        if let Ok((_, visual)) = visuals.get(entity) {
            commands.entity(visual.overlay).despawn();
            commands.entity(entity).remove::<FrozenVisual>();
        }

        if enemy_query.get(entity).is_ok() {
            commands.entity(entity).insert(JustThawed);
        }
    }
}

fn clear_frozen_on_death(
    mut commands: Commands,
    query: Query<(Entity, Option<&FrozenVisual>), (With<Frozen>, Added<Dead>)>,
) {
    for (entity, visual) in &query {
        if let Some(visual) = visual {
            commands.entity(visual.overlay).despawn();
            commands.entity(entity).remove::<FrozenVisual>();
        }
        commands.entity(entity).remove::<Frozen>();
    }
}
