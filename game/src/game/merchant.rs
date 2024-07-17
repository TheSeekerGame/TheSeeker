use theseeker_engine::animation::SpriteAnimationBundle;
use theseeker_engine::assets::animation::SpriteAnimation;
use theseeker_engine::gent::TransformGfxFromGent;
use theseeker_engine::prelude::*;
use theseeker_engine::script::ScriptPlayer;
use crate::prelude::*;

pub struct MerchantPlugin;

impl Plugin for MerchantPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            (setup_merchant.run_if(in_state(GameState::Playing))).run_if(in_state(AppState::InGame)),
        );
    }
}

#[derive(Component, Default)]
pub struct MerchantBlueprint;

#[derive(Bundle, LdtkEntity, Default)]
pub struct MerchantBlueprintBundle {
    marker: MerchantBlueprint,
}

#[derive(Component)]
pub struct MerchantGfx {
    pub e_gent: Entity,
}

#[derive(Bundle)]
pub struct MerchantGfxBundle {
    marker: MerchantGfx,
    gent2gfx: TransformGfxFromGent,
    sprite: SpriteSheetBundle,
    animation: SpriteAnimationBundle,
}

pub fn setup_merchant(
    mut q: Query<(&mut Transform, Entity), Added<MerchantBlueprint>>,
    mut commands: Commands,
) {
    for (mut xf_gent, e_gent) in q.iter_mut() {
        println!("added merchant");
        xf_gent.translation.z = 13.0 * 0.000001;
        println!("{:?}", xf_gent);
        let e_gfx = commands.spawn(()).id();
        commands.entity(e_gent).insert((
            Name::new("Merchant"),
        ));
        let mut player = ScriptPlayer::<SpriteAnimation>::default();
        player.play_key("anim.merchant.Idle");
        commands.entity(e_gfx).insert((MerchantGfxBundle {
            marker: MerchantGfx { e_gent },
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