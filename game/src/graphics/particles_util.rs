use bevy_hanabi::*;
use crate::prelude::{Vec3, Vec4};

pub fn color_gradient(keys: impl IntoIterator<Item = (f32, Vec4)>) -> Gradient<Vec4> {
    let mut gradient = Gradient::new();
    for (position, color) in keys { gradient.add_key(position, color); }
    gradient
}

pub struct CircleParticles {
    pub lifetime: f32,
    pub radius: f32,
    pub speed: f32,
    pub size: f32,
    pub colors: Gradient<Vec4>,
}
impl CircleParticles {
    pub fn effect(self, capacity: u32, spawner: SpawnerSettings) -> EffectAsset {
        let writer = ExprWriter::new();
        let age = SetAttributeModifier::new(Attribute::AGE, writer.lit(0.0).expr());
        let lifetime = SetAttributeModifier::new(Attribute::LIFETIME, writer.lit(self.lifetime).expr());
        let position = SetPositionCircleModifier {
            center: writer.lit(Vec3::ZERO).expr(), axis: writer.lit(Vec3::Z).expr(),
            radius: writer.lit(self.radius).expr(), dimension: ShapeDimension::Surface,
        };
        let velocity = SetVelocityCircleModifier {
            center: writer.lit(Vec3::ZERO).expr(), axis: writer.lit(Vec3::Z).expr(),
            speed: writer.lit(self.speed).expr(),
        };
        let mut module = writer.finish();
        let square_pixels = RoundModifier::constant(&mut module, 0.0);
        EffectAsset::new(capacity, spawner, module)
            .init(age).init(lifetime).init(position).init(velocity)
            .render(SizeOverLifetimeModifier { gradient: Gradient::constant(Vec3::splat(self.size)), screen_space_size: false })
            .render(ColorOverLifetimeModifier::new(self.colors)).render(square_pixels)
    }
}
