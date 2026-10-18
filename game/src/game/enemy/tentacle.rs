use bevy::asset::RenderAssetUsages;
use crate::game::actors::action::{ActiveAction, Mechanic};
use crate::game::actors::catalog::ActorCatalog;
use crate::game::actors::tick_strike::{ProceduralCombat, StrikePhase};
use theseeker_engine::animation::PlaybackId;
use bevy::mesh::{Mesh, Mesh2d};
use bevy::render::render_resource::PrimitiveTopology;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::sprite_render::{ColorMaterial, MeshMaterial2d};
use theseeker_engine::animation::SpriteAnimationBundle;
use theseeker_engine::gent::Gent;
use theseeker_engine::physics::{Collider, LinearVelocity};

use super::{Enemy, EnemyStateSet};
use crate::game::combat::{
    blood::{register_blood_sample_animation, BloodSampleSource},
    DamageSource, DespawnOnOwnerInvalid, Health,
};
use crate::game::effects::frozen::Frozen;
use crate::game::gentstate::{Dead, Facing};
use crate::game::player::Player;
use crate::graphics::pixel_perfect::pixel_perfect_layers;
use crate::prelude::*;

pub struct TentaclePlugin;

impl Plugin for TentaclePlugin {
    fn build(&self, app: &mut App) {
        register_blood_sample_animation(app, TENTACLE_HEAD_ANIM_KEY);
        app.add_systems(
            GameTickUpdate,
            setup_tentacles
                .before(EnemyStateSet::Actuator)
                .run_if(in_state(AppState::InGame))
                .run_if(in_state(GameState::Playing)),
        );

        app.add_systems(
            GameTickUpdate,
            tick_tentacles
                .in_set(EnemyStateSet::Actuator)
                .run_if(in_state(AppState::InGame)),
        );

        app.add_systems(
            GameTickUpdate,
            (
                prepare_dead_tentacle_corpses,
                fade_dead_tentacle_corpses,
            )
                .chain()
                .in_set(EnemyStateSet::Animation)
                .run_if(in_state(AppState::InGame)),
        );
    }
}

#[derive(Component, Default)]
pub struct TentacleBlueprint;

#[derive(Component)]
pub struct Tentacle;

#[derive(Component)]
struct TentacleVisual {
    mesh: Handle<Mesh>,
}

#[derive(Component)]
struct TentacleHeadGfx;

#[derive(Component)]
struct TentacleCorpseFadeComplete;

#[derive(Component)]
struct TentacleAttackHitbox;

type TentacleHeadQueryFilter = (
    With<TentacleHeadGfx>,
    Without<Tentacle>,
    Without<Player>,
    Without<TentacleAttackHitbox>,
);

type TentacleHitboxQueryFilter = (
    With<TentacleAttackHitbox>,
    Without<Tentacle>,
    Without<Player>,
    Without<TentacleHeadGfx>,
);

#[derive(Component)]
pub(super) struct TentacleState {
    phase: f32,
    joints: [Vec2; TENTACLE_JOINT_COUNT],
    motion_target: Vec2,
    attack_anchor: Vec2,
    attack_apex: Vec2,
    head_gfx: Option<Entity>,
    facing_target: f32,
    facing_blend: f32,
}

impl TentacleState {
    fn new(phase: f32) -> Self {
        let mut joints = [Vec2::ZERO; TENTACLE_JOINT_COUNT];
        for (idx, joint) in joints.iter_mut().enumerate() {
            *joint = Vec2::new(0.0, idx as f32 * TENTACLE_SEGMENT_LENGTH);
        }

        Self {
            phase,
            joints,
            motion_target: Vec2::new(
                -TENTACLE_IDLE_SIDE_BIAS_X,
                TENTACLE_IDLE_TARGET_Y,
            ),
            attack_anchor: Vec2::ZERO,
            attack_apex: Vec2::new(0.0, TENTACLE_ATTACK_APEX_MIN_Y),
            head_gfx: None,
            facing_target: -1.0,
            facing_blend: -1.0,
        }
    }

    fn tip(&self) -> Vec2 {
        self.joints[TENTACLE_SEGMENTS]
    }

    /// The tip in world space, for a tentacle rooted at `transform`.
    pub(super) fn tip_position(&self, transform: &Transform) -> Vec2 {
        transform.transform_point(self.tip().extend(0.0)).truncate()
    }
}

const TENTACLE_SEGMENTS: usize = 12;
const TENTACLE_JOINT_COUNT: usize = TENTACLE_SEGMENTS + 1;
const TENTACLE_REACH: f32 = 108.0;
const TENTACLE_SEGMENT_LENGTH: f32 = TENTACLE_REACH / TENTACLE_SEGMENTS as f32;
const TENTACLE_ARC_SOLVE_STEPS: usize = 18;
const TENTACLE_BASE_REVERSE_BEND: f32 = 0.42;
const TENTACLE_BASE_REVERSE_SPAN: f32 = 0.28;
const TENTACLE_TARGET_FOLLOW_IDLE: f32 = 0.055;
const TENTACLE_TARGET_FOLLOW_ATTACK: f32 = 0.34;
const TENTACLE_BODY_STIFFNESS_IDLE: f32 = 0.34;
const TENTACLE_BODY_STIFFNESS_ATTACK: f32 = 0.62;
const TENTACLE_FACING_SAMPLE_TICKS: u64 = 8;
const TENTACLE_FACING_DEAD_ZONE_X: f32 = 4.0;
const TENTACLE_FACING_FOLLOW: f32 = 0.045;
const TENTACLE_BASE_RADIUS: f32 = 7.0;
const TENTACLE_TIP_RADIUS: f32 = 2.2;
const TENTACLE_BODY_COLLIDER_SCALE: f32 = 0.28;
const TENTACLE_VISUAL_Y_OFFSET: f32 = -19.0;
const TENTACLE_GROUND_CLIP_Y: f32 = -3.0;
const TENTACLE_BODY_HEALTH: u32 = 180;
const TENTACLE_HITBOX_RADIUS: f32 = 7.0;
const TENTACLE_TRIGGER_X: f32 = 78.0;
const TENTACLE_TRIGGER_MIN_Y: f32 = -10.0;
const TENTACLE_TRIGGER_MAX_Y: f32 = 72.0;
const TENTACLE_MAX_STRIKE_X: f32 = 82.0;
const TENTACLE_ATTACK_APEX_OFFSET: f32 = 64.0;
const TENTACLE_ATTACK_APEX_MIN_Y: f32 = 58.0;
const TENTACLE_STRIKE_TARGET_Y_OFFSET: f32 = 3.0;
const TENTACLE_IDLE_SIDE_BIAS_X: f32 = 16.0;
const TENTACLE_IDLE_TARGET_Y: f32 = 58.0;
const TENTACLE_IDLE_SWAY_X: f32 = 0.0;
const TENTACLE_IDLE_SWAY_Y: f32 = 6.0;
const TENTACLE_CORPSE_FADE_TICKS: u32 = 96;
const TENTACLE_Z: f32 = 14.0 * 0.000_001;
const TENTACLE_HEAD_Z_OFFSET: f32 = 0.000_002;
const TENTACLE_HEAD_ANIM_KEY: &str = "anim.tentacle.Head";
const TENTACLE_LIGHT_COLOR: [f32; 4] = [0.58, 0.61, 0.61, 1.0];
const TENTACLE_DARK_COLOR: [f32; 4] = [0.31, 0.33, 0.34, 1.0];
const TENTACLE_CORPSE_LIGHT_COLOR: [f32; 4] = [0.44, 0.46, 0.46, 1.0];
const TENTACLE_CORPSE_DARK_COLOR: [f32; 4] = [0.38, 0.40, 0.40, 1.0];
const TENTACLE_MITER_LIMIT: f32 = 1.85;

#[derive(Clone, Copy)]
struct TentaclePalette {
    light: [f32; 4],
    dark: [f32; 4],
}

impl TentaclePalette {
    fn living() -> Self {
        Self {
            light: TENTACLE_LIGHT_COLOR,
            dark: TENTACLE_DARK_COLOR,
        }
    }

    fn corpse(fade: f32) -> Self {
        let fade = fade.clamp(0.0, 1.0);
        Self {
            light: lerp_color(
                TENTACLE_LIGHT_COLOR,
                TENTACLE_CORPSE_LIGHT_COLOR,
                fade,
            ),
            dark: lerp_color(
                TENTACLE_DARK_COLOR,
                TENTACLE_CORPSE_DARK_COLOR,
                fade,
            ),
        }
    }
}

fn setup_tentacles(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut query: Query<(Entity, &mut Transform), Added<TentacleBlueprint>>,
    level_seed: Res<crate::game::actors::LevelSeed>,
) {
    for (entity, mut transform) in query.iter_mut() {
        transform.translation.z = TENTACLE_Z;

        let phase =
            tentacle_phase(entity, transform.translation.xy(), level_seed.0);
        let mut state = TentacleState::new(phase);

        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        );
        rebuild_tentacle_mesh(
            &mut mesh,
            &state.joints,
            tentacle_shadow_side(&state),
            TentaclePalette::living(),
        );
        let mesh_handle = meshes.add(mesh);
        let material_handle = materials.add(ColorMaterial::default());

        let gfx_entity = commands
            .spawn((
                Name::new("TentacleGfx"),
                TentacleVisual {
                    mesh: mesh_handle.clone(),
                },
                Mesh2d(mesh_handle.clone()),
                MeshMaterial2d(material_handle),
                // The body mesh deforms far from this child entity's origin, so
                // Bevy's mesh bounds can cull it while pixels are still on-screen.
                NoFrustumCulling,
                Transform::default(),
                GlobalTransform::default(),
                Visibility::Visible,
                InheritedVisibility::default(),
                ViewVisibility::default(),
                bevy::ecs::hierarchy::ChildOf(entity),
                pixel_perfect_layers(),
                StateDespawnMarker,
            ))
            .id();
        let effects_entity = commands
            .spawn((
                Name::new("TentacleEffects"),
                Transform::default(),
                GlobalTransform::default(),
                bevy::ecs::hierarchy::ChildOf(entity),
                StateDespawnMarker,
            ))
            .id();
        let head_entity = commands
            .spawn((
                Name::new("TentacleHeadGfx"),
                TentacleHeadGfx,
                Sprite {
                    texture_atlas: Some(TextureAtlas::default()),
                    flip_y: tentacle_head_flip_y(&state),
                    ..Default::default()
                },
                tentacle_head_transform(&state),
                GlobalTransform::default(),
                Visibility::Visible,
                InheritedVisibility::default(),
                ViewVisibility::default(),
                SpriteAnimationBundle::new_play_key(TENTACLE_HEAD_ANIM_KEY),
                bevy::ecs::hierarchy::ChildOf(entity),
                pixel_perfect_layers(),
                StateDespawnMarker,
            ))
            .id();
        state.head_gfx = Some(head_entity);

        let mut collider = Collider::Empty;
        rebuild_tentacle_collider(&state.joints, &mut collider);

        commands
            .entity(entity)
            .insert((BloodSampleSource::animation(TENTACLE_HEAD_ANIM_KEY), ProceduralCombat::default()))
            .insert((
                Name::new("Tentacle"),
                Enemy,
                Tentacle,
                state,
                Gent {
                    e_gfx: gfx_entity,
                    e_effects_gfx: effects_entity,
                },
                collider,
                theseeker_engine::physics::groups::enemy_body(),
                LinearVelocity(Vec2::ZERO),
                Health {
                    current: TENTACLE_BODY_HEALTH,
                    max: TENTACLE_BODY_HEALTH,
                },
                Facing::Right,
                GlobalTransform::default(),
                Visibility::Visible,
                InheritedVisibility::default(),
                ViewVisibility::default(),
                StateDespawnMarker,
            ));

        commands.entity(entity).remove::<TentacleBlueprint>();
    }
}

#[expect(clippy::too_many_arguments, reason = "Bevy system dependencies are expressed as explicit system parameters.")]
fn tick_tentacles(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    time: Res<GameTime>,
    catalog: Res<ActorCatalog>,
    mut tentacles: Query<
        (
            Entity,
            &Transform,
            &mut TentacleState,
            &mut ProceduralCombat,
            &mut Collider,
            &Gent,
            Option<&Frozen>,
        ),
        (With<Tentacle>, Without<Dead>),
    >,
    visuals: Query<&TentacleVisual>,
    mut heads: Query<
        (&mut Transform, &mut Sprite),
        TentacleHeadQueryFilter,
    >,
    players: Query<&Transform, (With<Player>, Without<Dead>)>,
    mut hitboxes: Query<
        (&mut Transform, &mut Collider, &mut DamageSource),
        TentacleHitboxQueryFilter,
    >,
) {
    let player_pos = players.iter().next().map(|tf| tf.translation.xy());

    for (
        entity,
        transform,
        mut state,
        mut combat,
        mut body_collider,
        gent,
        frozen,
    ) in tentacles.iter_mut()
    {
        if frozen.is_some() {
            clear_procedural_hitbox(&mut combat, &mut commands);
            continue;
        }

        let base_pos = transform.translation.xy();
        let player_local = player_pos.map(|pos| pos - base_pos);
        let tick = time.tick();
        update_tentacle_facing(&mut state, player_local, tick);

        let idle_target = idle_target_for(&state, tick as f32);

        tick_idle_tentacle_state(entity, &mut state, &mut combat, player_local, &catalog);

        let desired_target = if let Some(action) = &combat.action {
            attack_target(&mut state, player_local, idle_target, action)
        } else { idle_target };
        let follow_strength = if combat.action.is_some() {
            TENTACLE_TARGET_FOLLOW_ATTACK
        } else {
            TENTACLE_TARGET_FOLLOW_IDLE
        };
        state.motion_target = state
            .motion_target
            .lerp(desired_target, follow_strength);

        let body_stiffness = if combat.action.is_some() {
            TENTACLE_BODY_STIFFNESS_ATTACK
        } else {
            TENTACLE_BODY_STIFFNESS_IDLE
        };
        let motion_target = state.motion_target;
        let facing_blend = state.facing_blend;
        follow_tip_with_stiffness(
            &mut state.joints,
            motion_target,
            body_stiffness,
            facing_blend,
        );

        rebuild_tentacle_collider(&state.joints, &mut body_collider);
        rebuild_tentacle_visual(
            &mut meshes,
            &visuals,
            gent.e_gfx,
            &state,
            TentaclePalette::living(),
        );
        sync_tentacle_head(&mut heads, &state);

        let strike_active = combat.action.as_ref().is_some_and(strike_is_active);
        let tip_world = base_pos + tentacle_display_point(state.tip());
        update_attack_hitbox(
            entity,
            &mut combat,
            strike_active,
            tip_world,
            &mut commands,
            &mut hitboxes,
        );

        finish_attack_if_complete(&mut combat, tick, &mut commands);
    }
}

fn prepare_dead_tentacle_corpses(
    mut commands: Commands,
    mut tentacles: Query<&mut ProceduralCombat, (With<Tentacle>, Added<Dead>)>,
) {
    for mut combat in tentacles.iter_mut() {
        if let Some(mut action) = combat.action.take() { action.interrupt(&mut commands); }
    }
}

fn fade_dead_tentacle_corpses(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    tentacles: Query<
        (Entity, &Dead, &Gent, &TentacleState),
        (With<Tentacle>, Without<TentacleCorpseFadeComplete>),
    >,
    visuals: Query<&TentacleVisual>,
) {
    for (entity, dead, gent, state) in tentacles.iter() {
        let fade_ticks = dead.ticks.min(TENTACLE_CORPSE_FADE_TICKS);
        let fade = fade_ticks as f32 / TENTACLE_CORPSE_FADE_TICKS as f32;
        rebuild_tentacle_visual(
            &mut meshes,
            &visuals,
            gent.e_gfx,
            state,
            TentaclePalette::corpse(fade),
        );

        if dead.ticks >= TENTACLE_CORPSE_FADE_TICKS {
            commands.entity(entity).insert(TentacleCorpseFadeComplete);
        }
    }
}

fn tick_idle_tentacle_state(entity: Entity, state: &mut TentacleState, combat: &mut ProceduralCombat, player_local: Option<Vec2>, catalog: &ActorCatalog) {
    if combat.action.is_some() { return; }
    combat.cooldown = combat.cooldown.saturating_sub(1);
    if combat.cooldown != 0 { return; }
    let Some(player_local) = player_local.filter(|target| should_attack(*target)) else { return; };
    let definition = catalog.actions.get("tentacle_strike").expect("validated tentacle strike").clone();
    combat.action = Some(crate::game::actors::action::ActionRequest { forward: 1.0, actor: entity, target: None, damage: definition.damage.expect("tentacle damage"), definition }.activate(PlaybackId::default()));
    enter_attack(state, player_local);
}

fn rebuild_tentacle_visual(
    meshes: &mut Assets<Mesh>,
    visuals: &Query<&TentacleVisual>,
    gfx_entity: Entity,
    state: &TentacleState,
    palette: TentaclePalette,
) {
    let Ok(visual) = visuals.get(gfx_entity) else {
        return;
    };
    let Some(mut mesh) = meshes.get_mut(&visual.mesh) else {
        return;
    };

    rebuild_tentacle_mesh(
        &mut mesh,
        &state.joints,
        tentacle_shadow_side(state),
        palette,
    );
}

fn sync_tentacle_head(
    heads: &mut Query<(&mut Transform, &mut Sprite), TentacleHeadQueryFilter>,
    state: &TentacleState,
) {
    let Some(head_gfx) = state.head_gfx else {
        return;
    };
    let Ok((mut head_transform, mut head_sprite)) = heads.get_mut(head_gfx)
    else {
        return;
    };

    *head_transform = tentacle_head_transform(state);
    head_sprite.flip_y = tentacle_head_flip_y(state);
}

fn finish_attack_if_complete(combat: &mut ProceduralCombat, tick: u64, commands: &mut Commands) {
    let Some(action) = &mut combat.action else { return; };
    if action.advance(tick, None, false).outcome.is_none() { return; }
    combat.cooldown = action.definition.timing.recovery_ticks;
    action.cleanup(commands);
    combat.action = None;
}

fn tentacle_phase(entity: Entity, pos: Vec2, level_seed: u32) -> f32 {
    let bits = entity.to_bits();
    let mut hash = ((bits ^ (bits >> 32)) as u32) ^ level_seed;
    hash ^= (pos.x.round() as i32 as u32).wrapping_mul(374_761_393);
    hash = hash.rotate_left(13);
    hash ^= (pos.y.round() as i32 as u32).wrapping_mul(668_265_263);
    let unit = hash as f32 / u32::MAX as f32;
    unit * std::f32::consts::TAU
}

fn idle_target_for(state: &TentacleState, tick: f32) -> Vec2 {
    let slow = tick * 0.016 + state.phase;
    let fast = tick * 0.028 + state.phase * 0.37;
    Vec2::new(
        state.facing_blend * TENTACLE_IDLE_SIDE_BIAS_X
            + slow.sin() * TENTACLE_IDLE_SWAY_X,
        TENTACLE_IDLE_TARGET_Y + fast.sin() * TENTACLE_IDLE_SWAY_Y,
    )
}

fn update_tentacle_facing(
    state: &mut TentacleState,
    player_local: Option<Vec2>,
    tick: u64,
) {
    if tick.is_multiple_of(TENTACLE_FACING_SAMPLE_TICKS) {
        if let Some(side) = player_facing_side(player_local) {
            state.facing_target = side;
        }
    }

    state.facing_blend = state
        .facing_blend
        .lerp(state.facing_target, TENTACLE_FACING_FOLLOW);
    if (state.facing_blend - state.facing_target).abs() <= 0.001 {
        state.facing_blend = state.facing_target;
    }
}

fn player_facing_side(player_local: Option<Vec2>) -> Option<f32> {
    let player_local = player_local?;
    if player_local.x < -TENTACLE_FACING_DEAD_ZONE_X {
        Some(-1.0)
    } else if player_local.x > TENTACLE_FACING_DEAD_ZONE_X {
        Some(1.0)
    } else {
        None
    }
}

fn should_attack(player_local: Vec2) -> bool {
    player_local.x.abs() <= TENTACLE_TRIGGER_X
        && player_local.y >= TENTACLE_TRIGGER_MIN_Y
        && player_local.y <= TENTACLE_TRIGGER_MAX_Y
}

fn enter_attack(state: &mut TentacleState, player_local: Vec2) {
    if let Some(side) = player_facing_side(Some(player_local)) {
        state.facing_target = side;
    }
    state.attack_anchor = clamp_attack_anchor(player_local);
    state.attack_apex = attack_apex_for(state.attack_anchor);
}

fn attack_target(state: &mut TentacleState, player_local: Option<Vec2>, idle_target: Vec2, action: &ActiveAction) -> Vec2 {
    let Mechanic::TentacleStrike { phases } = action.definition.mechanic else { unreachable!("tentacle action kind"); };
    match phases.phase(action.age) {
        StrikePhase::Windup => {
            if let Some(target) = player_local { state.attack_anchor = clamp_attack_anchor(target); }
            state.attack_apex = attack_apex_for(state.attack_anchor);
            state.attack_apex
        },
        StrikePhase::Strike(t) => state.attack_apex.lerp(strike_target_for(state.attack_anchor), ease_in_cubic(t)),
        StrikePhase::Recover(t) => strike_target_for(state.attack_anchor).lerp(idle_target, ease_out_cubic(t)),
    }
}

fn clamp_attack_anchor(player_local: Vec2) -> Vec2 {
    Vec2::new(
        player_local.x.clamp(-TENTACLE_MAX_STRIKE_X, TENTACLE_MAX_STRIKE_X),
        player_local
            .y
            .clamp(TENTACLE_TRIGGER_MIN_Y, TENTACLE_TRIGGER_MAX_Y),
    )
}

fn attack_apex_for(anchor: Vec2) -> Vec2 {
    Vec2::new(
        anchor.x,
        (anchor.y + TENTACLE_ATTACK_APEX_OFFSET).max(TENTACLE_ATTACK_APEX_MIN_Y),
    )
    .clamp_length_max(TENTACLE_REACH - TENTACLE_TIP_RADIUS)
}

fn strike_target_for(anchor: Vec2) -> Vec2 {
    Vec2::new(anchor.x, anchor.y + TENTACLE_STRIKE_TARGET_Y_OFFSET)
        .clamp_length_max(TENTACLE_REACH - TENTACLE_TIP_RADIUS)
}

fn strike_is_active(action: &ActiveAction) -> bool {
    let Mechanic::TentacleStrike { phases } = action.definition.mechanic else { return false; };
    phases.hit_active(action.age)
}

fn update_attack_hitbox(
    owner: Entity,
    combat: &mut ProceduralCombat,
    active: bool,
    tip_world: Vec2,
    commands: &mut Commands,
    hitboxes: &mut Query<
        (&mut Transform, &mut Collider, &mut DamageSource),
        TentacleHitboxQueryFilter,
    >,
) {
    if !active {
        clear_procedural_hitbox(combat, commands);
        return;
    }

    let Some(action) = &mut combat.action else { return; };
    let damage = action.definition.damage.expect("tentacle damage");
    if let Some(hitbox) = action.owned_entities.first().copied() {
        if let Ok((mut transform, mut collider, mut damage_source)) =
            hitboxes.get_mut(hitbox)
        {
            transform.translation = tip_world.extend(TENTACLE_Z + 0.000_001);
            *collider = Collider::ball(TENTACLE_HITBOX_RADIUS);
            damage_source.damage = damage;
            damage_source.base_damage = damage;
            return;
        }
        action.owned_entities.clear();
    }

    let hitbox = commands
        .spawn((
            TentacleAttackHitbox,
            DespawnOnOwnerInvalid,
            DamageSource::new(999, owner, damage).with_max_targets(1),
            Collider::ball(TENTACLE_HITBOX_RADIUS),
            theseeker_engine::physics::groups::enemy_attack(),
            Transform::from_translation(
                tip_world.extend(TENTACLE_Z + 0.000_001),
            ),
            GlobalTransform::default(),
            StateDespawnMarker,
            Name::new("TentacleStrikeHitbox"),
        ))
        .id();
    action.owned_entities.push(hitbox);
}

fn clear_procedural_hitbox(combat: &mut ProceduralCombat, commands: &mut Commands) {
    if let Some(action) = &mut combat.action { action.cleanup(commands); }
}

fn follow_tip_with_stiffness(
    joints: &mut [Vec2; TENTACLE_JOINT_COUNT],
    target: Vec2,
    stiffness: f32,
    facing_blend: f32,
) {
    let target = target.clamp_length_max(TENTACLE_REACH - TENTACLE_TIP_RADIUS);
    let mut left_solved = *joints;
    solve_elastic_arc_pose(&mut left_solved, target, 1.0);
    let mut right_solved = *joints;
    solve_elastic_arc_pose(&mut right_solved, target, -1.0);

    let right_mix =
        smoothstep(0.0, 1.0, (facing_blend.clamp(-1.0, 1.0) + 1.0) * 0.5);

    let stiffness = stiffness.clamp(0.0, 1.0);
    joints[0] = Vec2::ZERO;
    for idx in 1..TENTACLE_JOINT_COUNT {
        let solved = left_solved[idx].lerp(right_solved[idx], right_mix);
        joints[idx] = joints[idx].lerp(solved, stiffness);
    }
}

fn solve_elastic_arc_pose(
    joints: &mut [Vec2; TENTACLE_JOINT_COUNT],
    target: Vec2,
    bend_sign: f32,
) {
    let chord = target.length();
    let chord_dir = safe_dir(target, Vec2::Y);
    let chord_angle = chord_dir.y.atan2(chord_dir.x);
    let turn_scale = elastic_turn_scale_for_chord(chord);

    let mut local = [Vec2::ZERO; TENTACLE_JOINT_COUNT];
    build_elastic_local_pose(&mut local, turn_scale, bend_sign);

    let endpoint = local[TENTACLE_SEGMENTS];
    let endpoint_angle = endpoint.y.atan2(endpoint.x);
    let rotation = chord_angle - endpoint_angle;

    joints[0] = Vec2::ZERO;
    for idx in 1..TENTACLE_JOINT_COUNT {
        joints[idx] = rotate_vec2(local[idx], rotation);
    }

    joints[TENTACLE_SEGMENTS] = target;
}

fn build_elastic_local_pose(
    joints: &mut [Vec2; TENTACLE_JOINT_COUNT],
    turn_scale: f32,
    bend_sign: f32,
) {
    let mut angle: f32 = 0.0;
    joints[0] = Vec2::ZERO;

    for idx in 0..TENTACLE_SEGMENTS {
        let (sin, cos) = angle.sin_cos();
        let dir = Vec2::new(cos, sin);
        joints[idx + 1] = joints[idx] + dir * TENTACLE_SEGMENT_LENGTH;

        if idx < TENTACLE_SEGMENTS - 1 {
            angle += bend_sign * turn_scale * elastic_curvature_weight(idx);
        }
    }
}

fn elastic_curvature_weight(turn_idx: usize) -> f32 {
    let denom = (TENTACLE_SEGMENTS - 1).max(1) as f32;
    let t = turn_idx as f32 / denom;
    let reverse_fade = 1.0 - smoothstep(0.0, TENTACLE_BASE_REVERSE_SPAN, t);
    1.0 - reverse_fade * (1.0 + TENTACLE_BASE_REVERSE_BEND)
}

fn elastic_turn_scale_for_chord(chord: f32) -> f32 {
    let target_ratio =
        (chord / TENTACLE_SEGMENT_LENGTH).clamp(0.0, TENTACLE_SEGMENTS as f32);

    if target_ratio >= TENTACLE_SEGMENTS as f32 - 0.001 {
        return 0.0;
    }

    let mut low = 0.0;
    let mut high =
        std::f32::consts::TAU / TENTACLE_SEGMENTS as f32 - 0.0001;

    for _ in 0..6 {
        if elastic_chord_ratio_for_turn_scale(high) <= target_ratio {
            break;
        }
        high *= 1.35;
    }

    for _ in 0..TENTACLE_ARC_SOLVE_STEPS {
        let mid = (low + high) * 0.5;
        if elastic_chord_ratio_for_turn_scale(mid) > target_ratio {
            low = mid;
        } else {
            high = mid;
        }
    }

    high
}

fn elastic_chord_ratio_for_turn_scale(turn_scale: f32) -> f32 {
    let mut local = [Vec2::ZERO; TENTACLE_JOINT_COUNT];
    build_elastic_local_pose(&mut local, turn_scale, 1.0);
    local[TENTACLE_SEGMENTS].length() / TENTACLE_SEGMENT_LENGTH
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn lerp_color(from: [f32; 4], to: [f32; 4], t: f32) -> [f32; 4] {
    let t = t.clamp(0.0, 1.0);
    [
        from[0].lerp(to[0], t),
        from[1].lerp(to[1], t),
        from[2].lerp(to[2], t),
        from[3].lerp(to[3], t),
    ]
}

fn rotate_vec2(v: Vec2, radians: f32) -> Vec2 {
    let (sin, cos) = radians.sin_cos();
    Vec2::new(v.x * cos - v.y * sin, v.x * sin + v.y * cos)
}

fn tentacle_head_transform(state: &TentacleState) -> Transform {
    let tip = tentacle_display_point(state.tip());
    let dir = safe_dir(
        state.tip() - state.joints[TENTACLE_SEGMENTS - 1],
        Vec2::NEG_X,
    );
    let angle = dir.y.atan2(dir.x) - std::f32::consts::PI;
    Transform {
        translation: tip.extend(TENTACLE_HEAD_Z_OFFSET),
        rotation: Quat::from_rotation_z(angle),
        ..Default::default()
    }
}

fn tentacle_head_flip_y(state: &TentacleState) -> bool {
    state.facing_blend > 0.0
}

fn rebuild_tentacle_collider(
    joints: &[Vec2; TENTACLE_JOINT_COUNT],
    collider: &mut Collider,
) {
    let mut rects = Vec::with_capacity(TENTACLE_SEGMENTS);
    for idx in 0..TENTACLE_SEGMENTS {
        let a = tentacle_display_point(joints[idx]);
        let b = tentacle_display_point(joints[idx + 1]);
        let t = idx as f32 / TENTACLE_SEGMENTS as f32;
        let next_t = (idx + 1) as f32 / TENTACLE_SEGMENTS as f32;
        let radius = tentacle_radius(t).max(tentacle_radius(next_t))
            * TENTACLE_BODY_COLLIDER_SCALE;
        let mut min =
            Vec2::new(a.x.min(b.x), a.y.min(b.y)) - Vec2::splat(radius);
        let max = Vec2::new(a.x.max(b.x), a.y.max(b.y)) + Vec2::splat(radius);

        if max.y <= TENTACLE_GROUND_CLIP_Y {
            continue;
        }
        min.y = min.y.max(TENTACLE_GROUND_CLIP_Y);
        rects.push(Rect::from_corners(min, max));
    }

    *collider = if rects.is_empty() {
        Collider::Empty
    } else {
        Collider::multi_rect(rects)
    };
}

/// Rebuilds the tentacle's mesh. Each segment is three bands along its length: light, in
/// shadow toward `shadow_side`, and light again.
fn rebuild_tentacle_mesh(
    mesh: &mut Mesh,
    joints: &[Vec2; TENTACLE_JOINT_COUNT],
    shadow_side: f32,
    palette: TentaclePalette,
) {
    let shadow_center = shadow_side.clamp(-1.0, 1.0) * 0.5;
    let dark_min = (shadow_center - 0.5).clamp(-1.0, 1.0);
    let dark_max = (shadow_center + 0.5).clamp(-1.0, 1.0);
    // Each band spans the width from one side (-1) toward the other (1).
    let bands = [(-1.0, dark_min, palette.light), (dark_min, dark_max, palette.dark), (dark_max, 1.0, palette.light)];
    let mut positions = Vec::new();
    let mut uvs = Vec::new();
    let mut colors = Vec::new();
    for idx in 0..TENTACLE_SEGMENTS {
        let ends = [idx, idx + 1];
        let t = ends.map(|joint| joint as f32 / TENTACLE_SEGMENTS as f32);
        let centers = ends.map(|joint| tentacle_display_point(joints[joint]));
        let lefts = [0, 1].map(|end| miter_offset_at(joints, ends[end], tentacle_radius(t[end])));
        // Each band's quad: its `from` side at the segment's start and end, then its `to` side
        // at the end and start.
        let corners = bands.iter().filter(|(from, to, _)| to - from > 0.001).flat_map(|&(from, to, color)| {
            [(0, from), (1, from), (1, to), (0, to)].map(|(end, side)| (centers[end] + lefts[end] * side, t[end], color))
        });
        for (position, t, color) in corners {
            positions.push(clip_tentacle_visual_point(position).extend(0.0).to_array());
            uvs.push([0.5, t]);
            colors.push(color);
        }
    }
    let indices = (0..positions.len() as u32 / 4).flat_map(|quad| [0, 1, 2, 0, 2, 3].map(|corner| quad * 4 + corner));
    mesh.insert_indices(bevy::mesh::Indices::U32(indices.collect()));
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]; positions.len()]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
}

fn tentacle_shadow_side(state: &TentacleState) -> f32 {
    -state.facing_blend
}

fn tentacle_display_point(point: Vec2) -> Vec2 {
    point + Vec2::Y * TENTACLE_VISUAL_Y_OFFSET
}

fn clip_tentacle_visual_point(point: Vec2) -> Vec2 {
    Vec2::new(point.x, point.y.max(TENTACLE_GROUND_CLIP_Y))
}

fn miter_offset_at(
    joints: &[Vec2; TENTACLE_JOINT_COUNT],
    idx: usize,
    radius: f32,
) -> Vec2 {
    let segment_normal = |dir: Vec2| Vec2::new(-dir.y, dir.x);

    if idx == 0 {
        return Vec2::X * radius;
    }
    if idx == TENTACLE_SEGMENTS {
        let dir = safe_dir(
            joints[TENTACLE_SEGMENTS] - joints[TENTACLE_SEGMENTS - 1],
            Vec2::Y,
        );
        return segment_normal(dir) * radius;
    }

    let prev_dir = safe_dir(joints[idx] - joints[idx - 1], Vec2::Y);
    let next_dir = safe_dir(joints[idx + 1] - joints[idx], prev_dir);
    let prev_normal = segment_normal(prev_dir);
    let next_normal = segment_normal(next_dir);
    let miter = safe_dir(prev_normal + next_normal, prev_normal);
    let denom = miter.dot(next_normal).abs();
    let length = if denom <= 0.05 {
        radius
    } else {
        (radius / denom).min(radius * TENTACLE_MITER_LIMIT)
    };

    miter * length
}

fn tentacle_radius(t: f32) -> f32 {
    let taper = t.clamp(0.0, 1.0).powf(0.82);
    TENTACLE_BASE_RADIUS.lerp(TENTACLE_TIP_RADIUS, taper)
}

fn safe_dir(v: Vec2, fallback: Vec2) -> Vec2 {
    if v.length_squared() <= 0.0001 {
        fallback.normalize_or_zero()
    } else {
        v.normalize()
    }
}

fn ease_in_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * t
}

fn ease_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}
