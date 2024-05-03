use bevy::ecs::system::lifetimeless::*;
use bevy::ecs::system::SystemParam;

use crate::assets::animation::*;
use crate::assets::script::*;
use crate::data::OneOrMany;
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
    frame_actions: HashMap<u32, Vec<ActionId>>,
    framequant_actions: Vec<(Quant, ActionId)>,
    reversed: bool,
    next_frame: Option<u32>,
    frame_min: u32,
    frame_max: u32,
    ticks_per_frame: u32,
    ticks_remain: u32,
    bookmarks: HashMap<String, u32>,
}

impl SpriteAnimationTracker {
    fn resolve_bookmark(&self, bm: Option<&String>) -> u32 {
        let Some(bm) = bm else {
            return 0;
        };
        if let Some(i) = self.bookmarks.get(bm) {
            *i
        } else {
            if cfg!(feature = "dev") {
                warn!("Script bookmark {:?} is undefined!", bm);
            }
            0
        }
    }
    fn resolve_frame(&self, bm: Option<&String>, frame: &FrameIndexOrBookmark) -> u32 {
        let bm_offset = self.resolve_bookmark(bm);
        match frame {
            FrameIndexOrBookmark::Index(i) => *i + bm_offset,
            FrameIndexOrBookmark::Bookmark(bm) => self.resolve_bookmark(Some(bm)),
        }
    }
    fn set_next_frame(&mut self, index: u32) {
        if index < self.frame_min || index > self.frame_max {
            self.next_frame = None;
        } else {
            self.next_frame = Some(index);
        }
    }
    fn set_auto_next_frame(&mut self, current: u32) {
        self.next_frame = if self.reversed {
            if current > self.frame_min {
                Some(current - 1)
            } else { None }
        } else {
            if current < self.frame_max {
                Some(current + 1)
            } else { None }
        };
    }
}

impl ScriptRunIf for SpriteAnimationScriptRunIf {
    type Tracker = SpriteAnimationTracker;
}

impl ScriptActionParams for SpriteAnimationScriptParams {
    type Tracker = SpriteAnimationTracker;
    type ShouldRunParam = (
        SQuery<(
            &'static TextureAtlas,
        )>,
    );

    fn should_run<'w>(
        &self,
        entity: Entity,
        tracker: &mut Self::Tracker,
        _action_id: ActionId,
        (q_self,): &mut <Self::ShouldRunParam as SystemParam>::Item<'w, '_>,
    ) -> Result<(), ScriptUpdateResult> {
        let current_index = q_self.get(entity).unwrap().0.index as u32;
        if let Some(lt) = &self.if_frame_lt {
            if !(current_index < tracker.resolve_frame(self.frame_bookmark.as_ref(), lt)) {
                return Err(ScriptUpdateResult::NormalRun);
            }
        }
        if let Some(le) = &self.if_frame_le {
            if !(current_index <= tracker.resolve_frame(self.frame_bookmark.as_ref(), le)) {
                return Err(ScriptUpdateResult::NormalRun);
            }
        }
        if let Some(gt) = &self.if_frame_gt {
            if !(current_index > tracker.resolve_frame(self.frame_bookmark.as_ref(), gt)) {
                return Err(ScriptUpdateResult::NormalRun);
            }
        }
        if let Some(ge) = &self.if_frame_ge {
            if !(current_index >= tracker.resolve_frame(self.frame_bookmark.as_ref(), ge)) {
                return Err(ScriptUpdateResult::NormalRun);
            }
        }
        if let Some(f) = &self.if_frame_is {
            match f {
                OneOrMany::Single(f) => {
                    if current_index != tracker.resolve_frame(self.frame_bookmark.as_ref(), f) {
                        return Err(ScriptUpdateResult::NormalRun);
                    }
                }
                OneOrMany::Many(f) => {
                    for f in f.iter() {
                        if current_index != tracker.resolve_frame(self.frame_bookmark.as_ref(), f) {
                            return Err(ScriptUpdateResult::NormalRun);
                        }
                    }
                }
            }
        }
        if let Some(reversed) = self.if_playing_reversed {
            if reversed != tracker.reversed {
                return Err(ScriptUpdateResult::NormalRun);
            }
        }
        Ok(())
    }
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
        actionparams: &Self::ActionParams,
        tracker: &mut Self::Tracker,
        (q,): &mut <Self::Param as SystemParam>::Item<'w, '_>,
    ) -> ScriptUpdateResult {
        let (mut atlas, mut sprite, mut xf) = q
            .get_mut(entity)
            .expect("Entity is missing sprite animation components!");

        match self {
            SpriteAnimationScriptAction::SetFrameNext { to_frame_bookmark, frame_index } => {
                let bm_offset = tracker.resolve_bookmark(
                    to_frame_bookmark.as_ref()
                        .or(actionparams.frame_bookmark.as_ref())
                );
                tracker.set_next_frame(frame_index.unwrap_or(0) + bm_offset);
                ScriptUpdateResult::NormalRun
            },
            SpriteAnimationScriptAction::SetFrameNow { to_frame_bookmark, frame_index } => {
                let bm_offset = tracker.resolve_bookmark(
                    to_frame_bookmark.as_ref()
                        .or(actionparams.frame_bookmark.as_ref())
                );
                let index = frame_index.unwrap_or(0) + bm_offset;
                atlas.index = index as usize;
                tracker.set_auto_next_frame(index);
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
            SpriteAnimationScriptAction::ReversePlayback { reversed } => {
                let reversed = reversed.unwrap_or(!tracker.reversed);
                // On normal (contiguous) playback, adjust the next frame.
                // Otherwise (say if the script wants to jump around
                // using a SetFrameNext), do not touch it.
                match (tracker.reversed, reversed) {
                    (false, true) => {
                        tracker.reversed = true;
                        tracker.set_auto_next_frame(atlas.index as u32);
                    }
                    (true, false) => {
                        tracker.reversed = false;
                        tracker.set_auto_next_frame(atlas.index as u32);
                    }
                    _ => {}
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
    type InitParam = (SQuery<&'static mut TextureAtlas>,);
    type RunIf = SpriteAnimationScriptRunIf;
    type Settings = SpriteAnimationSettings;
    type UpdateParam = (SQuery<&'static mut TextureAtlas>,);
    type ActionParams = SpriteAnimationScriptParams;

    fn init<'w>(
        &mut self,
        entity: Entity,
        settings: &Self::Settings,
        _metadata: &ScriptMetadata,
        (q,): &mut <Self::InitParam as SystemParam>::Item<'w, '_>,
    ) {
        self.ticks_per_frame = settings.ticks_per_frame;
        self.ticks_remain = 0;
        self.next_frame = Some(settings.frame_start);
        self.frame_min = settings.frame_min;
        self.frame_max = settings.frame_max;
        self.reversed = settings.play_reversed;

        let mut atlas = q
            .get_mut(entity)
            .expect("Animation entity must have TextureAtlasSprite component");
        atlas.index = settings.frame_start as usize;
    }

    fn transfer_progress(&mut self, other: &Self) {
        self.ticks_remain = other.ticks_remain.min(self.ticks_per_frame);
    }

    fn finalize(&mut self) {
        // nothing really needs to be done here
    }

    fn track_action(
        &mut self,
        run_if: &Self::RunIf,
        params: &Self::ActionParams,
        action_id: ActionId,
    ) {
        let bm = params.frame_bookmark.as_ref();
        match run_if {
            SpriteAnimationScriptRunIf::Frame(frame) => {
                let mut actions = std::mem::take(&mut self.frame_actions);
                let mut add_action = |index| {
                    if let Some(e) = actions.get_mut(&index) {
                        e.push(action_id);
                    } else {
                        actions.insert(index, vec![action_id]);
                    }
                };
                match frame {
                    OneOrMany::Single(frame) => {
                        add_action(self.resolve_frame(bm, frame));
                    }
                    OneOrMany::Many(frames) => {
                        for frame in frames.iter() {
                            add_action(self.resolve_frame(bm, frame));
                        }
                    }
                };
                self.frame_actions = actions;
            },
            SpriteAnimationScriptRunIf::FrameQuant(quant) => {
                // adjust based on bookmark
                let bm_offset = self.resolve_bookmark(bm);
                let mut quant = *quant;
                quant.offset += bm_offset as i64;
                self.framequant_actions.push((quant, action_id));
            }
        }
    }

    fn update<'w>(
        &mut self,
        entity: Entity,
        _settings: &Self::Settings,
        (q,): &mut <Self::UpdateParam as SystemParam>::Item<'w, '_>,
        queue: &mut Vec<ActionId>,
    ) -> ScriptUpdateResult {
        let mut atlas = q
            .get_mut(entity)
            .expect("Animation entity must have TextureAtlasSprite component");

        if self.ticks_remain == 0 {
            let Some(next_frame) = self.next_frame else {
                return ScriptUpdateResult::Finished;
            };
            if let Some(actions) = self.frame_actions.get(&next_frame) {
                queue.extend_from_slice(&actions);
            }
            for (quant, action_id) in &self.framequant_actions {
                if quant.check(next_frame as i64) {
                    queue.push(*action_id);
                }
            }
            atlas.index = next_frame as usize;
            self.ticks_remain = self.ticks_per_frame;
            self.set_auto_next_frame(next_frame);
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

        let (h_image, h_layout) = self.resolve_image_atlas(&preloaded, builder.asset_key())
            .expect("Cannot resolve Animation asset's Image and Layout assets.");

        *image = h_image;
        atlas.layout = h_layout;

        atlas.index = self.settings.extended.frame_start
            .min(self.settings.extended.frame_max)
            .max(self.settings.extended.frame_min)
            as usize;

        builder.replace_config(&self.config);
        builder.tracker_mut().extended.bookmarks = self.frame_bookmarks.clone();
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
