use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::render_resource::*;

pub struct AbilityCooldownPlugin;

impl Plugin for AbilityCooldownPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiMaterialPlugin::<Material>::default());
    }
}

#[derive(Asset, TypePath, AsBindGroup, Clone, Copy, Debug)]
pub struct Material {
    /// A number between `0` and `1` indicating how much of the bar should be filled.
    #[uniform(0)]
    pub factor: f32,
    #[uniform(1)]
    pub background_color: LinearRgba,
    #[uniform(2)]
    pub filled_color: LinearRgba,
}

impl UiMaterial for Material {
    fn fragment_shader() -> ShaderRef {
        "shaders/ability_cooldown.wgsl".into()
    }
}
