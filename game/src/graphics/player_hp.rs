use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::render_resource::*;

use crate::game::attack::Health;
use crate::game::player::Player;
use crate::prelude::Update;

pub struct PlayerHpBarPlugin;

impl Plugin for PlayerHpBarPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiMaterialPlugin::<Material>::default());
        app.add_systems(Update, update_hp);
    }
}

#[derive(Component)]
pub struct Bar(pub Entity);

#[derive(Asset, TypePath, AsBindGroup, Clone, Copy, Debug)]
pub struct Material {
    /// A number between `0` and `1` indicating how much of the bar should be filled.
    #[uniform(0)]
    pub factor: f32,
    #[uniform(1)]
    pub background_color: Color,
    #[uniform(2)]
    pub filled_color: Color,
}

impl UiMaterial for Material {
    fn fragment_shader() -> ShaderRef {
        "shaders/player_hp.wgsl".into()
    }
}

fn update_hp(
    player_q: Query<&Health, With<Player>>,
    mut hp_bar_q: Query<(&Bar, &Handle<Material>)>,
    mut material: ResMut<Assets<Material>>,
) {
    for (hp_bar, material_handle) in hp_bar_q.iter() {
        if let Ok(health) = player_q.get(hp_bar.0) {
            if let Some(mat) = material.get_mut(material_handle) {
                mat.factor = 1.0 * (health.current as f32 / health.max as f32)
            }
        } else {
            if let Some(mat) = material.get_mut(material_handle) {
                mat.factor = 0.0;
            }
        }
    }
}
