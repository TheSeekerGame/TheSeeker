use bevy::ecs::system::lifetimeless::*;
use bevy::ecs::system::SystemParam;

use crate::assets::animation::*;
use crate::assets::script::*;
use crate::prelude::*;
use crate::script::common::ExtendedScriptTracker;
use crate::script::*;

pub struct SpriteAnimationPlugin;

impl Plugin for SpriteAnimationPlugin {
    fn build(&self, app: &mut App) {
        app.add_script_runtime::<SpriteAnimation>();
    }
}

#[derive(Default)]
pub struct SpriteAnimationTracker {
    next_frame: u32,
    ticks_per_frame: u32,
    ticks_remain: u32,
    frame_actions: HashMap<u32, ActionId>,
}

impl ScriptRunIf for SpriteAnimationScriptRunIf {
    type Tracker = SpriteAnimationTracker;
}

impl ScriptAction for SpriteAnimationScriptAction {
    type Param = (SQuery<(&'static mut TextureAtlasSprite, &'static mut Transform)>,);
    type Tracker = SpriteAnimationTracker;

    fn run<'w>(
        &self,
        entity: Entity,
        tracker: &mut Self::Tracker,
        (q,): &mut <Self::Param as SystemParam>::Item<'w, '_>,
    ) -> ScriptUpdateResult {
        let (mut sprite, mut xf) = q
            .get_mut(entity)
            .expect("Entity is missing sprite animation components!");

        match self {
            SpriteAnimationScriptAction::SetFrameNext { frame_index } => {
                tracker.next_frame = *frame_index;
                ScriptUpdateResult::NormalRun
            },
            SpriteAnimationScriptAction::SetFrameNow { frame_index } => {
                sprite.index = *frame_index as usize;
                ScriptUpdateResult::Loop
            },
            SpriteAnimationScriptAction::SetTicksPerFrame { ticks_per_frame } => {
                tracker.ticks_per_frame = *ticks_per_frame;
                tracker.ticks_remain = tracker.ticks_remain.min(*ticks_per_frame);
                ScriptUpdateResult::NormalRun
            },
            SpriteAnimationScriptAction::SetSpriteColor { color } => {
                sprite.color = (*color).into();
                ScriptUpdateResult::NormalRun
            },
            SpriteAnimationScriptAction::SetSpriteFlip { flip_x, flip_y } => {
                if let Some(flip_x) = flip_x {
                    sprite.flip_x = *flip_x;
                }
                if let Some(flip_y) = flip_y {
                    sprite.flip_y = *flip_y;
                }
                ScriptUpdateResult::NormalRun
            },
            SpriteAnimationScriptAction::TransformMove { x, y, z } => {
                if let Some(x) = x {
                    xf.translation.x += f32::from(*x);
                }
                if let Some(y) = y {
                    xf.translation.y += f32::from(*y);
                }
                if let Some(z) = z {
                    xf.translation.z += f32::from(*z);
                }
                ScriptUpdateResult::NormalRun
            }
            SpriteAnimationScriptAction::TransformTeleport { x, y, z } => {
                xf.translation.x = f32::from(*x);
                xf.translation.y = f32::from(*y);
                if let Some(z) = z {
                    xf.translation.z = f32::from(*z);
                }
                ScriptUpdateResult::NormalRun
            }
            SpriteAnimationScriptAction::TransformSetScale { x, y } => {
                xf.scale.x = f32::from(*x);
                xf.scale.y = f32::from(*y);
                ScriptUpdateResult::NormalRun
            }
            SpriteAnimationScriptAction::TransformRotateDegrees { degrees } => {
                xf.rotate_z(f32::from(*degrees).to_radians());
                ScriptUpdateResult::NormalRun
            }
            SpriteAnimationScriptAction::TransformRotateTurns { turns } => {
                xf.rotate_z(f32::from(*turns) * 2.0 * std::f32::consts::PI);
                ScriptUpdateResult::NormalRun
            }
            SpriteAnimationScriptAction::TransformSetRotationDegrees { degrees } => {
                xf.rotation = Quat::from_rotation_z(f32::from(*degrees).to_radians());
                ScriptUpdateResult::NormalRun
            }
            SpriteAnimationScriptAction::TransformSetRotationTurns { turns } => {
                xf.rotation = Quat::from_rotation_z(f32::from(*turns) * 2.0 * std::f32::consts::PI);
                ScriptUpdateResult::NormalRun
            }
        }
    }
}

impl ScriptTracker for SpriteAnimationTracker {
    type InitParam = ();
    type RunIf = SpriteAnimationScriptRunIf;
    type Settings = SpriteAnimationSettings;
    type UpdateParam = (SQuery<&'static mut TextureAtlasSprite>,);

    fn init<'w>(
        &mut self,
        _entity: Entity,
        settings: &Self::Settings,
        _param: &mut <Self::InitParam as SystemParam>::Item<'w, '_>,
    ) {
        self.ticks_per_frame = settings.ticks_per_frame;
        self.ticks_remain = 0;
        self.next_frame = settings.frame_start;
    }

    fn transfer_progress(&mut self, other: &Self) {
        self.ticks_remain = other.ticks_remain.min(self.ticks_per_frame);
    }

    fn finalize(&mut self) {
        // nothing really needs to be done here
    }

    fn track_action(&mut self, run_if: &Self::RunIf, action_id: ActionId) {
        match run_if {
            SpriteAnimationScriptRunIf::Frame(frame) => {
                self.frame_actions.insert(*frame, action_id);
            },
        }
    }

    fn update<'w>(
        &mut self,
        entity: Entity,
        settings: &Self::Settings,
        (q,): &mut <Self::UpdateParam as SystemParam>::Item<'w, '_>,
        queue: &mut Vec<ActionId>,
    ) -> ScriptUpdateResult {
        let mut sprite = q
            .get_mut(entity)
            .expect("Animation entity must have TextureAtlasSprite component");
        // if our sprite index was changed externally, respond to that by
        // running any frame actions and queueing up the next frame appropriately
        if sprite.is_changed() && !sprite.is_added() {
            self.next_frame = sprite.index as u32 + 1;
            if let Some(action_id) = self.frame_actions.get(&(sprite.index as u32)) {
                queue.push(*action_id);
            }
        }

        if self.next_frame > settings.frame_max || self.next_frame < settings.frame_min {
            return ScriptUpdateResult::Finished;
        }

        if self.ticks_remain == 0 {
            if let Some(action_id) = self.frame_actions.get(&self.next_frame) {
                queue.push(*action_id);
            }
            sprite.index = self.next_frame as usize;
            self.next_frame += 1;
            self.ticks_remain = self.ticks_per_frame;
        }

        self.ticks_remain -= 1;

        ScriptUpdateResult::NormalRun
    }
}

impl ScriptAsset for SpriteAnimation {
    type Action = ExtendedScriptAction<SpriteAnimationScriptAction>;
    type BuildParam = (
        SQuery<&'static mut Handle<TextureAtlas>>,
        SRes<PreloadedAssets>,
    );
    type RunIf = ExtendedScriptRunIf<SpriteAnimationScriptRunIf>;
    type Settings = ExtendedScriptSettings<SpriteAnimationSettings>;
    type Tracker = ExtendedScriptTracker<SpriteAnimationTracker>;

    fn into_settings(&self) -> Self::Settings {
        self.settings.clone()
    }

    fn build<'w>(
        &self,
        mut builder: ScriptRuntimeBuilder<Self>,
        entity: Entity,
        (q_atlas, preloaded): &mut <Self::BuildParam as SystemParam>::Item<'w, '_>,
    ) -> ScriptRuntimeBuilder<Self> {
        let mut atlas = q_atlas
            .get_mut(entity)
            .expect("Animation entity must have Texture Atlas component");
        let new_atlas = preloaded
            .get_single_asset(&self.atlas_asset_key)
            .expect("Invalid texture atlas asset key for animation");
        *atlas = new_atlas;

        for action in self.script.iter() {
            builder = builder.add_action(&action.run_if, &action.action);
        }
        builder
    }
}
