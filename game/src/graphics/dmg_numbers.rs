//! Damage numbers: pixel digits that rise from a damaged actor and fade, pausing with the game.
use bevy::color::palettes::css;
use theseeker_engine::animation::SpriteAnimationBundle;
use theseeker_engine::audio::transport::TICK_RATE_HZ;
use theseeker_engine::physics::Collider;
use theseeker_engine::script::ScriptPlayer;

use crate::game::combat::damage_source::apply_damage;
use crate::game::combat::DamageInfo;
use crate::game::player::Player;
use crate::prelude::*;

/// Damage numbers stay hidden while this is off.
const DAMAGE_NUMBERS_ENABLED: bool = false;
/// How fast a number rises, in world units per second of game time, when it starts fading,
/// and when it is gone.
const RISE_SPEED: f32 = 6.0;
const FADE_START: f32 = 0.25;
const LIFETIME: f32 = 3.0;
/// How far above its target's collider a number appears.
const ABOVE_COLLIDER: f32 = 15.0;
/// A digit's width, and the step from one digit to the next.
const DIGIT_WIDTH: f32 = 3.0;
const DIGIT_STEP: f32 = 4.0;
/// Each number lies this much above the one before, until a second passes without one.
const STACK_STEP: f32 = 0.000_000_1;

pub struct DmgNumbersPlugin;

impl Plugin for DmgNumbersPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DamageNumberStack>()
            .add_systems(GameTickUpdate, spawn_damage_numbers.after(apply_damage))
            .add_systems(Update, rise_and_fade);
    }
}

/// Marks an entity whose damage shows no numbers.
#[derive(Component)]
pub struct NoDamageNumbers;

/// How far above the world the next number lies, so later numbers cover earlier ones.
#[derive(Resource, Default)]
struct DamageNumberStack {
    z: f32,
    last_spawn_tick: u64,
}

#[derive(Component)]
struct DamageDigit {
    start_y: f32,
    /// When the digit appeared, in game seconds.
    spawned_at: f64,
}

fn spawn_damage_numbers(
    mut commands: Commands,
    mut damage: MessageReader<DamageInfo>,
    targets: Query<(&GlobalTransform, Option<&Collider>, Has<Player>, Has<NoDamageNumbers>)>,
    time: Res<GameTime>,
    mut stack: ResMut<DamageNumberStack>,
) {
    if !DAMAGE_NUMBERS_ENABLED {
        return;
    }
    for info in damage.read() {
        let Ok((transform, collider, player, silent)) = targets.get(info.target) else {
            continue;
        };
        if silent {
            continue;
        }
        let color = match (player, info.crit, info.stealthed) {
            (true, ..) => css::RED.into(),
            (false, true, true) => css::PURPLE.into(),
            (false, true, false) => (css::YELLOW * 1.1).into(),
            (false, false, true) => css::PINK.into(),
            (false, false, false) => Color::WHITE,
        };
        let above = collider.map_or(Vec3::ZERO, |collider| Vec3::new(1.0, collider.half_extents().y + ABOVE_COLLIDER, 0.0));
        let position = transform.translation() + above;
        if time.tick() > stack.last_spawn_tick + u64::from(TICK_RATE_HZ) {
            stack.z = 0.0;
        }
        stack.z += STACK_STEP;
        stack.last_spawn_tick = time.tick();
        let text = format!("{:.0}", info.amount);
        let left = position.x - (text.len() as f32 * DIGIT_STEP - (DIGIT_STEP - DIGIT_WIDTH)) / 2.0;
        for (i, digit) in text.chars().enumerate() {
            let mut player = ScriptPlayer::default();
            player.play_key(&format!("anim.numbers.{digit}"));
            commands.spawn((
                DamageDigit { start_y: position.y, spawned_at: time.time_in_seconds() },
                SpriteAnimationBundle { player },
                Sprite { texture_atlas: Some(default()), color, ..default() },
                Transform::from_xyz(left + i as f32 * DIGIT_STEP, position.y, position.z + 0.1 + stack.z),
                StateDespawnMarker,
            ));
        }
    }
}

fn rise_and_fade(mut commands: Commands, time: Res<GameTime>, mut digits: Query<(Entity, &DamageDigit, &mut Transform, &mut Sprite)>) {
    for (entity, digit, mut transform, mut sprite) in &mut digits {
        let age = (time.time_in_seconds() - digit.spawned_at) as f32;
        transform.translation.y = digit.start_y + RISE_SPEED * age;
        sprite.color.set_alpha(1.0 - ((age - FADE_START) / (LIFETIME - FADE_START)).clamp(0.0, 1.0));
        if age > LIFETIME {
            commands.entity(entity).despawn();
        }
    }
}
