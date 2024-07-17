use theseeker_engine::animation::SpriteAnimationBundle;
use theseeker_engine::assets::animation::SpriteAnimation;
use theseeker_engine::gent::TransformGfxFromGent;
use theseeker_engine::prelude::*;
use theseeker_engine::script::ScriptPlayer;
use crate::prelude::*;

pub struct YakPlugin;

impl Plugin for YakPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            (setup_yak.run_if(in_state(GameState::Playing))).run_if(in_state(AppState::InGame)),
        );
    }
}

#[derive(Component, Default)]
pub struct YakBlueprint;

#[derive(Bundle, LdtkEntity, Default)]
pub struct YakBlueprintBundle {
    marker: YakBlueprint,
}

#[derive(Component)]
pub struct YakGfx {
    pub e_gent: Entity,
}

#[derive(Bundle)]
pub struct YakGfxBundle {
    marker: YakGfx,
    gent2gfx: TransformGfxFromGent,
    sprite: SpriteSheetBundle,
    animation: SpriteAnimationBundle,
}

pub fn setup_yak(
    mut q: Query<(&mut Transform, Entity), Added<YakBlueprint>>,
    mut commands: Commands,
) {
    for (mut xf_gent, e_gent) in q.iter_mut() {
        println!("added yak");
        xf_gent.translation.z = 12.0 * 0.000001;
        xf_gent.translation.y += 5.0;
        println!("{:?}", xf_gent);
        let e_gfx = commands.spawn(()).id();
        commands.entity(e_gent).insert((
            Name::new("Yak"),
        ));
        let mut player = ScriptPlayer::<SpriteAnimation>::default();
        player.play_key("anim.yak.Idle");
        commands.entity(e_gfx).insert((YakGfxBundle {
            marker: YakGfx { e_gent },
            gent2gfx: TransformGfxFromGent {
                pixel_aligned: false,
                gent: e_gent,
            },
            sprite: SpriteSheetBundle {
                transform: *xf_gent,
                ..Default::default()
            },
            animation: SpriteAnimationBundle { player },
        },));
    }
}