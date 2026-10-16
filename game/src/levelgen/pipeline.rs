//! Reproducible inputs and explicit generation lifetime.
use super::*;
use rand::Rng;

#[derive(Resource, Clone)]
pub(crate) struct GenerationRequest {
    pub seed: u64,
    pub map: Option<String>,
    pub config: LevelPngConfig,
}

impl GenerationRequest {
    pub(crate) fn random(config: LevelPngConfig) -> Self {
        let seed = std::env::var("SEEKER_LEVEL_SEED").ok().and_then(|s| s.parse().ok())
            .unwrap_or_else(|| rand::rng().random());
        Self { seed, map: std::env::var("SEEKER_LEVEL_MAP").ok(), config }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub(crate) enum BuildPhase {
    #[default]
    Assets,
    Ground,
    Surfaces,
    Publishing,
    Ready,
    Failed(String),
}

#[derive(Resource)]
pub(crate) struct GenerationBuild {
    pub id: u64,
    pub phase: BuildPhase,
}

impl Default for GenerationBuild {
    fn default() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        Self { id: NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed), phase: BuildPhase::Assets }
    }
}

impl GenerationBuild {
    pub fn ground_ready(&self) -> bool { matches!(self.phase, BuildPhase::Ground | BuildPhase::Surfaces | BuildPhase::Publishing | BuildPhase::Ready) }
    pub fn surfaces_ready(&self) -> bool { matches!(self.phase, BuildPhase::Surfaces | BuildPhase::Publishing | BuildPhase::Ready) }
    pub fn fail(&mut self, error: String) {
        error!("generation {} failed: {error}", self.id);
        self.phase = BuildPhase::Failed(error);
    }
}

pub(super) fn publish_ready(world: &mut World) {
    let ready = world.resource::<GenerationBuild>().phase == BuildPhase::Surfaces
        && world.resource::<TileCollisionWorld>().is_ready()
        && crate::game::enemy::spawn_catalog_ready(world)
        && crate::graphics::post_processing::darkness::bake_ready(world);
    let id = world.resource::<GenerationBuild>().id;
    if ready {
        diagnostics::record_cpu_ready(world);
        publication::request(world, id);
        world.resource_mut::<GenerationBuild>().phase = BuildPhase::Publishing;
    }
    if world.resource::<GenerationBuild>().phase == BuildPhase::Publishing && publication::ready(world, id) {
        world.resource_mut::<GenerationBuild>().phase = BuildPhase::Ready;
    }
}

pub(super) fn report_asset_failures(mut failures: MessageReader<bevy::asset::AssetLoadFailedEvent<Image>>,
    mut build: ResMut<GenerationBuild>,
) {
    for failure in failures.read() {
        if matches!(build.phase, BuildPhase::Ready | BuildPhase::Failed(_)) { continue; }
        build.fail(format!("{}: {}", failure.path, failure.error));
    }
}
