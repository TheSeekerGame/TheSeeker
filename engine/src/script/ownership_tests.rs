use super::*;
use crate::assets::animation::SpriteAnimation;

fn live_player() -> ScriptPlayer<SpriteAnimation> {
    let clip: SpriteAnimation = toml::from_str(include_str!("../../../assets/animations/player/skills/DownwardDash.anim.toml")).unwrap();
    ScriptPlayer { state: ScriptPlayerState::Playing { runtime: ScriptRuntime {
        key: Some("active".to_owned()), config: ScriptConfig::default(),
        settings: clip.settings, actions: Vec::new(), tracker: crate::script::common::ExtendedScriptTracker::default(),
    } }, ..ScriptPlayer::default() }
}

#[test]
fn superseding_uncommitted_requests_preserves_the_active_runtime() {
    let mut player = live_player();
    player.play_key("first_request");
    player.play_handle(Handle::default());
    player.play_key("final_request");
    let ScriptPlayerState::ChangingKey { key, old_runtime } = &player.state else { panic!("active runtime retained"); };
    assert_eq!(key, "final_request");
    assert_eq!(old_runtime.key.as_deref(), Some("active"));
    player.stop();
    assert!(matches!(player.state, ScriptPlayerState::Stopping { .. }));
}

#[test]
fn foreground_release_restores_the_latest_background_request_only_for_its_owner() {
    let mut player = ScriptPlayer::<SpriteAnimation>::default();
    player.request_background("Idle");
    player.acquire_presentation(1);
    player.play_key("action");
    let execution = player.execution_id();
    player.request_background("Run");
    assert_eq!(player.execution_id(), execution);
    player.acquire_presentation(2);
    assert!(!player.release_presentation(1));
    assert_eq!(player.execution_id(), execution);
    assert!(player.release_presentation(2));
    assert!(matches!(&player.state, ScriptPlayerState::PrePlayKey { key, .. } if key == "Run"));
    assert!(!player.release_presentation(2));
}

#[test]
fn presentation_facing_slots_keep_native_orientation_and_restore_background_intent() {
    let mut player = ScriptPlayer::<SpriteAnimation>::default();
    player.set_slot("DirectionRight", true);
    player.acquire_presentation(1);
    player.set_presentation_slot(1, "DirectionRight", false);
    player.set_slot("DirectionRight", true);
    assert!(!player.has_slot("DirectionRight"));
    assert!(!player.release_presentation(2));
    assert!(!player.has_slot("DirectionRight"));
    assert!(player.release_presentation(1));
    assert!(player.has_slot("DirectionRight"));
}
