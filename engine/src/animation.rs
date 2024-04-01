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

#[derive(Bundle, Default)]
pub struct SpriteAnimationBundle {
    pub player: ScriptPlayer<SpriteAnimation>,
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

impl ScriptActionParams for SpriteAnimationScriptParams {
    type Tracker = SpriteAnimationTracker;
    type ShouldRunParam = ();
}

impl ScriptAction for SpriteAnimationScriptAction {
    type ActionParams = SpriteAnimationScriptParams;
    type Param = (
        SQuery<(
            &'static mut TextureAtlas,
            &'static mut Sprite,
            &'static mut Transform,
        )>,
    );
    type Tracker = SpriteAnimationTracker;

    fn run<'w>(
        &self,
        entity: Entity,
        _actionparams: &Self::ActionParams,
        tracker: &mut Self::Tracker,
        (q,): &mut <Self::Param as SystemParam>::Item<'w, '_>,
    ) -> ScriptUpdateResult {
        let (mut atlas, mut sprite, mut xf) = q
            .get_mut(entity)
            .expect("Entity is missing sprite animation components!");

        match self {
            SpriteAnimationScriptAction::SetFrameNext { frame_index } => {
                tracker.next_frame = *frame_index;
                ScriptUpdateResult::NormalRun
            },
            SpriteAnimationScriptAction::SetFrameNow { frame_index } => {
                atlas.index = *frame_index as usize;
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
            },
            SpriteAnimationScriptAction::TransformTeleport { x, y, z } => {
                xf.translation.x = f32::from(*x);
                xf.translation.y = f32::from(*y);
                if let Some(z) = z {
                    xf.translation.z = f32::from(*z);
                }
                ScriptUpdateResult::NormalRun
            },
            SpriteAnimationScriptAction::TransformSetScale { x, y } => {
                xf.scale.x = f32::from(*x);
                xf.scale.y = f32::from(*y);
                ScriptUpdateResult::NormalRun
            },
            SpriteAnimationScriptAction::TransformRotateDegrees { degrees } => {
                xf.rotate_z(f32::from(*degrees).to_radians());
                ScriptUpdateResult::NormalRun
            },
            SpriteAnimationScriptAction::TransformRotateTurns { turns } => {
                xf.rotate_z(f32::from(*turns) * 2.0 * std::f32::consts::PI);
                ScriptUpdateResult::NormalRun
            },
            SpriteAnimationScriptAction::TransformSetRotationDegrees { degrees } => {
                xf.rotation = Quat::from_rotation_z(f32::from(*degrees).to_radians());
                ScriptUpdateResult::NormalRun
            },
            SpriteAnimationScriptAction::TransformSetRotationTurns { turns } => {
                xf.rotation = Quat::from_rotation_z(f32::from(*turns) * 2.0 * std::f32::consts::PI);
                ScriptUpdateResult::NormalRun
            },
        }
    }
}

impl ScriptTracker for SpriteAnimationTracker {
    type InitParam = ();
    type RunIf = SpriteAnimationScriptRunIf;
    type Settings = SpriteAnimationSettings;
    type UpdateParam = (SQuery<&'static mut TextureAtlas>,);

    fn init<'w>(
        &mut self,
        _entity: Entity,
        settings: &Self::Settings,
        _metadata: &ScriptMetadata,
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
        let mut atlas = q
            .get_mut(entity)
            .expect("Animation entity must have TextureAtlasSprite component");

        // if our sprite index was changed externally, respond to that by
        // running any frame actions and queueing up the next frame appropriately
        if atlas.is_changed() && !atlas.is_added() {
            self.next_frame = atlas.index as u32 + 1;
            if let Some(action_id) = self.frame_actions.get(&(atlas.index as u32)) {
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
            atlas.index = self.next_frame as usize;
            self.next_frame += 1;
            self.ticks_remain = self.ticks_per_frame;
        }

        self.ticks_remain -= 1;

        ScriptUpdateResult::NormalRun
    }

    fn set_slot(&mut self, _slot: &str, _state: bool) {
    }
}

impl ScriptAsset for SpriteAnimation {
    type Action = ExtendedScriptAction<SpriteAnimationScriptAction>;
    type ActionParams = ExtendedScriptParams<SpriteAnimationScriptParams>;
    type BuildParam = (
        SQuery<(
            &'static mut Handle<Image>,
            &'static mut TextureAtlas,
            &'static mut Sprite,
        )>,
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
        let (mut image, mut atlas, mut _sprite) = q_atlas
            .get_mut(entity)
            .expect("Animation entity must have Texture Atlas components");

        *image = if let Some(key) = &self.settings.extended.image_asset_key {
            preloaded.get_single_asset(key)
        } else if let Some(key) = builder.asset_key() {
            let mut key = key.to_owned();
            key.push_str(".image");
            preloaded.get_single_asset(&key)
        } else {
            panic!("Unknown asset key for Animation Sprite Sheet Image")
        }.expect("Animation Sprite Sheet Image asset with specified key does not exist");

        atlas.layout = if let Some(key) = &self.settings.extended.atlas_asset_key {
            preloaded.get_single_asset(key)
        } else if let Some(key) = builder.asset_key() {
            let mut key = key.to_owned();
            key.push_str(".atlas");
            preloaded.get_single_asset(&key)
        } else {
            panic!("Unknown asset key for Animation Texture Atlas Layout")
        }.expect("Animation Texture Atlas Layout asset with specified key does not exist");

        atlas.index = self.settings.extended.frame_start
            .min(self.settings.extended.frame_max)
            .max(self.settings.extended.frame_min)
            as usize;

        builder.replace_config(&self.config);
        for action in self.script.iter() {
            builder = builder.add_action(
                &action.run_if,
                &action.action,
                &action.params,
            );
        }

        builder
    }
}
