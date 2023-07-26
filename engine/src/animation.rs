use crate::assets::animation::*;
use crate::prelude::*;

pub struct AnimationPlugin;

impl Plugin for AnimationPlugin {
    fn build(&self, app: &mut App) {
        app.add_system(sprite_animation_update.in_set(GameTickSet::Pre));
    }
}

#[derive(Component)]
pub struct SpriteAnimationPlayer {
    animation: Handle<SpriteAnimation>,
    state: SpriteAnimationPlayerState,
}

enum SpriteAnimationPlayerState {
    Stopped,
    PendingPlay {
        settings: PlaySettings,
    },
    Playing {
        settings: PlaySettings,
        scripts: PlayScripts,
        state: PlayState,
    },
}

#[derive(Debug, Clone)]
struct PlaySettings {
    paused: PauseMode,
}

#[derive(Debug)]
struct PlayState {
    starting_tick: u64,
    frameid_last: u64,
    tick_last: u64,
    ticks_per_frame: u32,
    next_index: u32,
    next_tick_script_index: usize,
    autoresume: Option<u64>,
}

#[derive(Default)]
struct PlayScripts {
    tick: Vec<(u64, SpriteAnimationActionKind)>,
    frame: HashMap<u32, Vec<SpriteAnimationActionKind>>,
    tickquant: Vec<(TickQuant, SpriteAnimationActionKind)>,
}

impl SpriteAnimationPlayer {
    pub fn play(&mut self, animation: Handle<SpriteAnimation>) {
        self.animation = animation;
        self.state = SpriteAnimationPlayerState::PendingPlay {
            settings: PlaySettings {
                paused: PauseMode::Playing,
            },
        };
    }

    pub fn stop(&mut self) {
        self.state = SpriteAnimationPlayerState::Stopped;
    }

    pub fn playing(&self) -> bool {
        match &self.state {
            SpriteAnimationPlayerState::Stopped => false,
            SpriteAnimationPlayerState::Playing { settings, .. } => {
                settings.paused == PauseMode::Playing
            },
            SpriteAnimationPlayerState::PendingPlay { .. } => false,
        }
    }

    /// Is playback currently paused?
    pub fn paused(&self) -> PauseMode {
        match &self.state {
            SpriteAnimationPlayerState::Stopped => PauseMode::NoScripts,
            SpriteAnimationPlayerState::Playing { settings, .. } => settings.paused,
            SpriteAnimationPlayerState::PendingPlay { settings, .. } => settings.paused,
        }
    }

    /// Control the pause state
    pub fn set_paused(&mut self, value: PauseMode) {
        match &mut self.state {
            SpriteAnimationPlayerState::Stopped => {},
            SpriteAnimationPlayerState::Playing {
                ref mut settings, ..
            } => {
                settings.paused = value;
            },
            SpriteAnimationPlayerState::PendingPlay {
                ref mut settings, ..
            } => {
                settings.paused = value;
            },
        }
    }

    /// Unpause (continue playback)
    pub fn resume(&mut self) {
        self.set_paused(PauseMode::Playing);
    }
}

fn import_scripts(play_scripts: &mut PlayScripts, anim_scripts: &[SpriteAnimationAction]) {
    for action in anim_scripts {
        match action {
            SpriteAnimationAction::AtTick { tick, action } => {
                play_scripts.tick.push((*tick, action.clone()));
            },
            SpriteAnimationAction::AtFrame {
                frame_index,
                action,
            } => {
                if let Some(actions) = play_scripts.frame.get_mut(frame_index) {
                    actions.push(action.clone());
                } else {
                    play_scripts
                        .frame
                        .insert(*frame_index, vec![action.clone()]);
                }
            },
            SpriteAnimationAction::EveryNTicks { quant, action } => {
                play_scripts.tickquant.push((*quant, action.clone()));
            },
        }
    }
    // must sort the tick scripts, so we can advance them efficiently
    play_scripts.tick.sort_unstable_by_key(|x| x.0);
}

enum ScriptActionResult {
    Ok,
    Stop,
}

fn process_script_action(
    settings: &mut PlaySettings,
    state: &mut PlayState,
    sprite: &mut TextureAtlasSprite,
    action: &SpriteAnimationActionKind,
    tick: u64,
) -> ScriptActionResult {
    match action {
        SpriteAnimationActionKind::Stop => {
            return ScriptActionResult::Stop;
        },
        SpriteAnimationActionKind::SetTicksPerFrame { ticks_per_frame } => {
            state.ticks_per_frame = *ticks_per_frame;
        },
        SpriteAnimationActionKind::SetPaused {
            mode,
            duration_ticks,
        } => {
            settings.paused = *mode;
            if let Some(duration_ticks) = duration_ticks {
                state.autoresume = Some(tick + *duration_ticks as u64);
            }
        },
        SpriteAnimationActionKind::SetFrameNow { frame_index } => {
            sprite.index = *frame_index as usize;
        },
        SpriteAnimationActionKind::SetFrameNext { frame_index } => {
            state.next_index = *frame_index;
        },
        SpriteAnimationActionKind::SetSpriteColor { color } => {
            sprite.color = *color;
        },
    }
    ScriptActionResult::Ok
}

fn sprite_animation_update(
    gametime: Res<GameTime>,
    preloaded: Res<PreloadedAssets>,
    ass_animation: Res<Assets<SpriteAnimation>>,
    mut q: Query<(
        &mut SpriteAnimationPlayer,
        &mut TextureAtlasSprite,
        &mut Handle<TextureAtlas>,
    )>,
) {
    for (mut player, mut sprite, mut atlas_handle) in &mut q {
        // if the asset is unavailable, do nothing
        let Some(animation) = ass_animation.get(&player.animation) else {
            continue;
        };
        // try to begin playback if not started yet
        if let SpriteAnimationPlayerState::PendingPlay { settings } = &player.state {
            if let Some(atlas) = preloaded.get_single_asset(&animation.atlas_asset_key) {
                *atlas_handle = atlas;
                sprite.index = animation.settings.frame_index_start as usize;
                let starting_tick = match animation.settings.tick_mode {
                    TickMode::Relative => gametime.tick(),
                    TickMode::RelativeQuantized(quant) => quant.apply(gametime.tick()),
                    TickMode::Absolute => 0,
                };
                let mut scripts = PlayScripts::default();
                import_scripts(&mut scripts, &animation.script);
                player.state = SpriteAnimationPlayerState::Playing {
                    settings: settings.clone(),
                    scripts,
                    state: PlayState {
                        starting_tick,
                        frameid_last: 0,
                        tick_last: 0,
                        next_tick_script_index: 0,
                        ticks_per_frame: animation.settings.ticks_per_frame,
                        next_index: animation.settings.frame_index_start + 1,
                        autoresume: None,
                    },
                };
            } else {
                error!(
                    "Failed to play animation, because atlas {:?} is not preloaded!",
                    animation.atlas_asset_key
                );
                player.stop();
            }
        }
        // process playing animations
        if let SpriteAnimationPlayerState::Playing {
            ref mut settings,
            ref mut scripts,
            ref mut state,
        } = &mut player.state
        {
            // calculate ticks relative to start
            let tick = gametime.tick() - state.starting_tick;
            // in "ticks_per_frame" units (let's call that frameid)
            let frameid_now = tick / state.ticks_per_frame as u64;

            let mut stop = false;

            if let Some(autoresume) = state.autoresume {
                if tick >= autoresume {
                    settings.paused = PauseMode::Playing;
                }
            }

            // skip this animation if it is paused
            if settings.paused == PauseMode::NoScripts {
                continue;
            }

            // process any tick scripts
            loop {
                if let Some(entry) = scripts.tick.get(state.next_tick_script_index) {
                    if entry.0 > tick {
                        break;
                    } else {
                        match process_script_action(
                            settings,
                            state,
                            &mut sprite,
                            &entry.1,
                            tick,
                        ) {
                            ScriptActionResult::Ok => {},
                            ScriptActionResult::Stop => {
                                stop = true;
                            },
                        }
                        state.next_tick_script_index += 1;
                    }
                } else {
                    break;
                }
            }
            // process any periodic tick scripts
            for (quant, action) in &scripts.tickquant {
                let quant_last = quant.convert(state.tick_last);
                let quant_this = quant.convert(tick);
                for _ in 0..(quant_this - quant_last) {
                    match process_script_action(
                        settings,
                        state,
                        &mut sprite,
                        action,
                        tick,
                    ) {
                        ScriptActionResult::Ok => {},
                        ScriptActionResult::Stop => {
                            stop = true;
                        },
                    }
                }
            }

            // advance frames
            if settings.paused == PauseMode::Playing {
                // check if enough ticks have passed to advance to the next frame
                if frameid_now > state.frameid_last {
                    // process any frame scripts
                    if let Some(actions) = scripts.frame.get(&state.next_index) {
                        for action in actions {
                            match process_script_action(
                                settings,
                                state,
                                &mut sprite,
                                action,
                                tick,
                            ) {
                                ScriptActionResult::Ok => {},
                                ScriptActionResult::Stop => {
                                    stop = true;
                                },
                            }
                        }
                    }
                    // change the sprite index, but avoid spurious bevy change detection
                    if sprite.index != state.next_index as usize {
                        sprite.index = state.next_index as usize;
                    }
                    state.next_index += 1;
                    // stop if we have reached the marked end frame
                    if state.next_index > animation.settings.frame_index_end {
                        stop = true;
                    }
                }
            }

            state.tick_last = tick;

            if stop {
                player.stop();
            }
        }
    }
}
