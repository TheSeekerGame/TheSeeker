use rapier2d::prelude::InteractionGroups;
use theseeker_engine::assets::animation::SpriteAnimation;
use theseeker_engine::gent::{Gent, TransformGfxFromGent};
use theseeker_engine::physics::{PhysicsWorld, PLAYER, SENSOR};
use theseeker_engine::script::ScriptPlayer;
use theseeker_engine::{animation::SpriteAnimationBundle, physics::Collider};

use crate::prelude::*;

use super::player::{Idle, Player};

pub struct MerchantPlugin;

impl Plugin for MerchantPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            ((
                setup_merchant
                    .run_if(any_matching::<Added<MerchantBlueprint>>()),
                merchant_proximity_to_player.run_if(any_matching::<(
                    With<Player>,
                    Without<Idle>,
                )>()),
            )
                .run_if(
                    in_state(GameState::Playing)
                        .and_then(in_state(AppState::InGame)),
                )),
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
        let e_effects_gfx = commands.spawn_empty().id();
        commands.entity(e_gent).insert((
            Name::new("Merchant"),
            Gent {
                e_gfx,
                e_effects_gfx,
            },
            Collider::cuboid(
                40.0,
                40.0,
                InteractionGroups {
                    memberships: SENSOR,
                    filter: PLAYER,
                },
            ),
        ));
        let mut player = ScriptPlayer::<SpriteAnimation>::default();
        player.play_key("anim.merchant.Idle");
        commands.entity(e_gfx).insert((
            MerchantGfxBundle {
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
            },
            StateDespawnMarker,
        ));
    }
}

fn merchant_proximity_to_player(
    merchant_query: Query<
        (
            Entity,
            &Gent,
            &GlobalTransform,
            &Collider,
        ),
        With<MerchantBlueprint>,
    >,
    mut animation_query: Query<
        &mut ScriptPlayer<SpriteAnimation>,
        With<MerchantGfx>,
    >,
    spatial_query: Res<PhysicsWorld>,
) {
    for (entity, gent, transform, collider) in merchant_query.iter() {
        let intersections = spatial_query.intersect(
            transform.translation().xy(),
            collider.0.shape(),
            collider.0.collision_groups(),
            Some(entity),
        );

        if let Ok(mut animation) = animation_query.get_mut(gent.e_gfx) {
            let is_player_nearby = !intersections.is_empty();
            animation.set_slot("PlayerNearby", is_player_nearby);
        }
    }
}
