//! Headless validation of pass scheduling, shader composition, uniform layouts, and (on a
//! host with a GPU adapter) rendered frames.
use bevy::prelude::*;
use bevy::camera::{RenderTarget, Viewport, visibility::RenderLayers};
use bevy::core_pipeline::{Core2d, Core2dSystems, CorePipelinePlugin, FullscreenShader};
use bevy::reflect::{PartialReflect, ReflectRef};
use bevy::render::RenderApp;
use bevy::render::error_handler::RenderErrorHandler;
use bevy::render::gpu_readback::{Readback, ReadbackComplete};
use bevy::render::render_resource::{CachedPipelineState, DownlevelFlags, PipelineCache, PipelineDescriptor, ShaderType,
    TextureFormat, TextureUsages, WgpuFeatures, encase::{UniformBuffer, internal::CreateFrom}};
use bevy::shader::{ShaderCache, ShaderCacheSource, ShaderLoader};
use naga::{ArraySize, Module, TypeInner};
use std::sync::{Arc, Mutex};
use crate::{appstate::AppState, camera::MainCamera};
use super::pixel_perfect::PixelPerfectCompositeSettings;
use super::post_processing::capsule_dim::CapsuleDimSettings;
use super::post_processing::darkness::DarknessSettings;
use super::post_processing::foreground_fog::ForegroundFogSettings;
use super::post_processing::vignette::VignetteSettings;
use super::ripple::RippleUniform;

#[test]
fn custom_render_passes_initialize_with_bevys_per_view_schedule() {
    let mut app = App::new();
    let mut render = SubApp::new();
    render.add_schedule(Core2d::base_schedule());
    render.add_systems(Core2d,
        bevy::core_pipeline::tonemapping::tonemapping.in_set(Core2dSystems::PostProcess));
    app.insert_sub_app(RenderApp, render);
    super::assemble_render_passes(&mut app);
    app.sub_app_mut(RenderApp).world_mut().schedule_scope(Core2d, |world, schedule| {
        schedule.initialize(world).expect("custom passes have compatible parameters and acyclic ordering");
    });
}

#[test]
fn fullscreen_shaders_compose_and_read_their_uniforms_where_rust_writes_them() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, bevy::asset::AssetPlugin::default()));
    app.init_asset::<Image>().init_asset::<Shader>().init_asset_loader::<ShaderLoader>();
    app.add_plugins(CorePipelinePlugin);
    app.init_resource::<FullscreenShader>();
    let vertex = app.world().resource::<FullscreenShader>().shader();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !app.world().resource::<Assets<Shader>>().contains(vertex.id()) {
        assert!(std::time::Instant::now() < deadline, "embedded fullscreen shader did not load");
        app.update();
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    let mut cache = ShaderCache::new((), WgpuFeatures::empty(), DownlevelFlags::all(), |(), source, _| match source {
        ShaderCacheSource::Naga(module) => Ok(module),
        _ => unreachable!("Bevy composes WGSL into naga modules"),
    });
    let mut assets = app.world_mut().resource_mut::<Assets<Shader>>();
    cache.set_shader(vertex.id(), assets.get(&vertex).unwrap().clone());
    // Each fullscreen shader, its uniform structs, and the Rust uniforms written into them.
    let uniforms: [(&str, &str, fn(&Module, &str)); 6] = [
        ("pixel_perfect_composite.wgsl", "Settings", assert_laid_out_as::<PixelPerfectCompositeSettings>),
        ("post_processing/darkness.wgsl", "DarknessSettings", assert_laid_out_as::<DarknessSettings>),
        ("post_processing/capsule_dim.wgsl", "CapsuleDimSettings", assert_laid_out_as::<CapsuleDimSettings>),
        ("post_processing/ripple.wgsl", "RippleSettings", assert_laid_out_as::<RippleUniform>),
        ("post_processing/vignette.wgsl", "VignetteSettings", assert_laid_out_as::<VignetteSettings>),
        ("post_processing/vignette.wgsl", "ForegroundFogSettings", assert_laid_out_as::<ForegroundFogSettings>),
    ];
    for (path, name, assert_laid_out) in uniforms {
        let full_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/shaders").join(path);
        let shader = Shader::from_wgsl(std::fs::read_to_string(full_path).unwrap(), path);
        let handle = assets.add(shader.clone());
        cache.set_shader(handle.id(), shader);
        let module = cache.get(0, handle.id(), &[]).unwrap_or_else(|error| panic!("{path}: {error:?}"));
        assert_laid_out(&module, name);
    }
}

/// Checks that the uniform `T` lays out as the struct `name` in `shader`: the same fields in
/// the same order, each where the shader reads it, and the same size.
fn assert_laid_out_as<T: ShaderType + CreateFrom + PartialReflect>(shader: &Module, name: &str) {
    let (ty, span) = shader.types.iter().find_map(|(handle, ty)| match ty.inner {
        TypeInner::Struct { span, .. } if ty.name.as_deref() == Some(name) => Some((handle, span)),
        _ => None,
    }).unwrap_or_else(|| panic!("the shader has no struct {name}"));
    assert_eq!(u64::from(span), T::min_size().get(), "{name} size");
    // Every word holds its own index, so each field reads back the words it lies over.
    let words: Vec<u8> = (0..span / 4).flat_map(u32::to_le_bytes).collect();
    let uniform: T = UniformBuffer::new(words).create().unwrap();
    assert_lies_at(shader, ty, 0, &uniform, name);
}

/// Checks that `value` holds the indices of the words the shader's type `ty` spans from `offset`.
fn assert_lies_at(shader: &Module, ty: naga::Handle<naga::Type>, offset: u32, value: &dyn PartialReflect, path: &str) {
    match (&shader.types[ty].inner, value.reflect_ref()) {
        (TypeInner::Struct { members, .. }, ReflectRef::Struct(fields)) => {
            let names: Vec<_> = fields.iter_fields().map(|(name, _)| Some(name)).collect();
            assert_eq!(members.iter().map(|member| member.name.as_deref()).collect::<Vec<_>>(), names, "{path} fields");
            for (member, (name, field)) in members.iter().zip(fields.iter_fields()) {
                assert_lies_at(shader, member.ty, offset + member.offset, field, &format!("{path}.{name}"));
            }
        },
        (&TypeInner::Array { base, size: ArraySize::Constant(len), stride }, ReflectRef::Array(elements)) => {
            assert_eq!(elements.len(), len.get() as usize, "{path} length");
            for (index, element) in elements.iter().enumerate() {
                assert_lies_at(shader, base, offset + index as u32 * stride, element, &format!("{path}[{index}]"));
            }
        },
        (inner, _) => {
            let words = offset / 4..(offset + inner.size(shader.to_ctx())) / 4;
            assert_eq!(scalars(value), words.collect::<Vec<_>>(), "{path}");
        },
    }
}

/// The bits of a scalar, or of each component of a vector.
fn scalars(value: &dyn PartialReflect) -> Vec<u32> {
    match value.reflect_ref() {
        ReflectRef::Struct(components) => components.iter_fields().flat_map(|(_, component)| scalars(component)).collect(),
        _ => vec![value.try_downcast_ref::<f32>().map(|x| x.to_bits()).or_else(|| value.try_downcast_ref::<u32>().copied())
            .unwrap_or_else(|| panic!("{} is no shader scalar", value.reflect_type_path()))],
    }
}

/// A windowless app whose main camera renders into a 64×64 image, and the image's latest
/// GPU readback. Tests using it need an adapter, so they are ignored by default.
pub(crate) struct Headless {
    pub(crate) app: App,
    pub(crate) output: Handle<Image>,
    pixels: Arc<Mutex<Vec<u8>>>,
}

impl Headless {
    /// An app whose `setup` adds the plugins and render passes under test, entering the game
    /// with `camera` as its main camera.
    pub(crate) fn new(camera: Camera, setup: impl FnOnce(&mut App)) -> Self {
        let mut app = App::new();
        app.add_plugins(DefaultPlugins
            .set(WindowPlugin { primary_window: None, exit_condition: bevy::window::ExitCondition::DontExit, ..default() })
            .set(bevy::render::RenderPlugin { synchronous_pipeline_compilation: true, ..default() })
            .disable::<bevy::winit::WinitPlugin>()
            .disable::<bevy::audio::AudioPlugin>()
            .disable::<bevy::render::pipelined_rendering::PipelinedRenderingPlugin>());
        app.init_state::<AppState>();
        // A GPU validation error fails the test with its description rather than stopping rendering.
        app.insert_resource(RenderErrorHandler(|error, _, _| panic!("{error:?}")));
        setup(&mut app);
        app.finish();
        app.cleanup();
        let mut output = Image::new_target_texture(64, 64, TextureFormat::Rgba8UnormSrgb, None);
        output.texture_descriptor.usage |= TextureUsages::COPY_SRC;
        let output = app.world_mut().resource_mut::<Assets<Image>>().add(output);
        app.world_mut().spawn((Camera2d, MainCamera, RenderTarget::Image(output.clone().into()), camera, RenderLayers::layer(0)));
        let pixels = Arc::new(Mutex::new(Vec::<u8>::new()));
        let received = pixels.clone();
        app.world_mut().spawn(Readback::texture(output.clone())).observe(move |event: On<ReadbackComplete>| {
            *received.lock().unwrap() = event.data.clone();
        });
        app.world_mut().resource_mut::<NextState<AppState>>().set(AppState::InGame);
        Self { app, output, pixels }
    }

    pub(crate) fn run(&mut self, frames: usize) {
        for _ in 0..frames {
            self.app.update();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    /// The pixel at `x`, `y` from the top left of the latest readback.
    pub(crate) fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let pixels = self.pixels.lock().unwrap();
        assert!(!pixels.is_empty(), "GPU readback did not complete");
        pixels[(y * 64 + x) as usize * 4..][..4].try_into().unwrap()
    }

    pub(crate) fn center(&self) -> [u8; 4] {
        self.pixel(32, 32)
    }
}

/// A main camera cleared to `color`.
pub(crate) fn cleared_to(color: Color) -> Camera {
    Camera { clear_color: ClearColorConfig::Custom(color), ..default() }
}

#[test]
#[ignore = "requires a GPU adapter; run explicitly without a window or audio"]
fn pixel_layer_reaches_the_composited_output() {
    use super::pixel_perfect::{PixelPerfectPlugin, PixelPerfectTarget, pixel_composite_pass};
    use super::pixel_view::{PixelView, PixelViewMapping};

    let mut headless = Headless::new(cleared_to(Color::BLACK), |app| {
        app.insert_resource(PixelView(Some(PixelViewMapping::new(Vec3::ZERO, 1, UVec2::splat(64)))));
        app.add_plugins(PixelPerfectPlugin);
        app.sub_app_mut(RenderApp).add_systems(Core2d, pixel_composite_pass.in_set(Core2dSystems::PostProcess));
    });
    let world = headless.app.world_mut();
    world.spawn((Camera2d, RenderTarget::Image(headless.output.clone().into()),
        Camera { order: 1, clear_color: ClearColorConfig::None, ..default() }, RenderLayers::layer(2)));
    let mesh = world.resource_mut::<Assets<Mesh>>().add(Rectangle::new(32.0, 32.0));
    let material = world.resource_mut::<Assets<ColorMaterial>>().add(Color::srgb(1.0, 0.0, 0.0));
    let platform = world.spawn((Mesh2d(mesh), MeshMaterial2d(material), RenderLayers::layer(3))).id();
    headless.run(100);
    assert_eq!(headless.app.world().resource::<PixelPerfectTarget>().size, UVec2::splat(64));
    assert_red_center(&headless);
    let old_mesh = headless.app.world().get::<Mesh2d>(platform).unwrap().0.id();
    replace_test_platform(&mut headless.app, platform, super::quad_mesh::QuadMesh::default().finish());
    headless.run(30);
    assert!(headless.app.sub_app(RenderApp).world().resource::<bevy::render::render_asset::RenderAssets<bevy::render::mesh::RenderMesh>>()
        .get(old_mesh).is_none(), "empty geometry retained its GPU allocation");
    assert_eq!(headless.center(), [0, 0, 0, 255]);
    replace_test_platform(&mut headless.app, platform, Mesh::from(Rectangle::new(32.0, 32.0)));
    headless.run(30);
    assert_red_center(&headless);
}

fn assert_red_center(headless: &Headless) {
    let center = headless.center();
    assert!(center[0] > 200 && center[1] < 10 && center[2] < 10,
        "pixel-layer red platform was not composited: {center:?}");
}

fn replace_test_platform(app: &mut App, entity: Entity, mesh: Mesh) {
    app.world_mut().resource_scope(|world, mut meshes: Mut<Assets<Mesh>>| {
        let mut handle = world.get_mut::<Mesh2d>(entity).unwrap();
        super::quad_mesh::publish_mesh(&mut meshes, &mut handle.0, mesh);
    });
}

const LETTERBOX_BLACK: [u8; 4] = [0, 0, 0, 255];
const VIEW_BLUE: [u8; 4] = [0, 0, 255, 255];

/// A main camera cleared to blue and letterboxed as in the game to 56×52 pixels in the middle
/// of the image, which at two screen pixels per game pixel shows 28×26 game pixels, and its
/// pixel view.
fn letterboxed_view() -> (Camera, super::pixel_view::PixelView) {
    let viewport = Viewport { physical_position: UVec2::new(4, 6), physical_size: UVec2::new(56, 52), ..default() };
    let view = super::pixel_view::PixelViewMapping::new(Vec3::ZERO, 2, viewport.physical_size);
    let camera = Camera { viewport: Some(viewport), output_mode: crate::camera::LETTERBOX, ..cleared_to(Color::srgb(0.0, 0.0, 1.0)) };
    (camera, super::pixel_view::PixelView(Some(view)))
}

#[test]
#[ignore = "requires a GPU adapter; run explicitly without a window or audio"]
fn letterboxed_view_shows_whole_layer_texels_between_black_bars() {
    use super::pixel_perfect::{PixelPerfectPlugin, pixel_composite_pass};

    let (camera, view) = letterboxed_view();
    let mut headless = Headless::new(camera, |app| {
        app.insert_resource(view).add_plugins(PixelPerfectPlugin);
        app.sub_app_mut(RenderApp).add_systems(Core2d, pixel_composite_pass.in_set(Core2dSystems::PostProcess));
    });
    let world = headless.app.world_mut();
    let mesh = world.resource_mut::<Assets<Mesh>>().add(Rectangle::new(8.0, 8.0));
    let material = world.resource_mut::<Assets<ColorMaterial>>().add(Color::srgb(1.0, 0.0, 0.0));
    world.spawn((Mesh2d(mesh), MeshMaterial2d(material), Transform::from_xyz(3.0, 2.0, 0.0), RenderLayers::layer(3)));
    headless.run(100);
    // The layer's texels center on the camera, so world x −1..7 is texel columns 13..21 and
    // world y 6..−2 is rows 7..15, each texel two screen pixels past the viewport's corner.
    let red = |x: u32, y: u32| (4 + 2 * 13..4 + 2 * 21).contains(&x) && (6 + 2 * 7..6 + 2 * 15).contains(&y);
    let shown = |x: u32, y: u32| (4..60).contains(&x) && (6..58).contains(&y);
    for (x, y) in (0..64).flat_map(|x| (0..64).map(move |y| (x, y))) {
        let expected = if !shown(x, y) { LETTERBOX_BLACK } else if red(x, y) { [255, 0, 0, 255] } else { VIEW_BLUE };
        assert_eq!(headless.pixel(x, y), expected, "pixel ({x}, {y})");
    }
}

#[test]
#[ignore = "requires a GPU adapter; run explicitly without a window or audio"]
fn every_fullscreen_pass_compiles_and_draws_in_a_letterboxed_view() {
    use crate::game::player::spawns::tether_node::{TetherInfluenceVisuals, VisualInfluenceCircle};

    let (camera, view) = letterboxed_view();
    let mut headless = Headless::new(camera, |app| {
        // A tether off the view's center draws its protection into the fog's mask.
        let tether = VisualInfluenceCircle { center: Vec2::new(-9.0, 7.0), radius: 3.0, deformation: 0.0 };
        app.insert_resource(view).insert_resource(TetherInfluenceVisuals { circles: vec![tether] })
            .init_resource::<crate::camera::GameplayPixelScale>()
            .init_resource::<theseeker_engine::time::GameTime>().init_resource::<theseeker_engine::time::PresentationTime>();
        app.add_plugins((super::pixel_perfect::PixelPerfectPlugin, super::post_processing::PostProcessingPlugin,
            super::ripple::RippleFxPlugin));
        super::assemble_render_passes(app);
    });
    headless.run(30);
    // Only the fog that the vignette pass lays over the view tints its blue.
    assert_ne!(headless.center(), VIEW_BLUE, "the last pass left the view untouched");
    // Every render pipeline, named by its fragment shader's path or else its label.
    let server = headless.app.world().resource::<AssetServer>();
    let pipelines: Vec<_> = headless.app.sub_app(RenderApp).world().resource::<PipelineCache>().pipelines()
        .filter_map(|pipeline| match &pipeline.descriptor {
            PipelineDescriptor::RenderPipelineDescriptor(descriptor) => Some((descriptor, &pipeline.state)),
            PipelineDescriptor::ComputePipelineDescriptor(_) => None,
        })
        .map(|(descriptor, state)| {
            let path = descriptor.fragment.as_ref().and_then(|fragment| server.get_path(fragment.shader.id()));
            (path.map_or_else(|| descriptor.label.as_deref().unwrap_or_default().to_owned(), |path| path.to_string()), state)
        })
        .collect();
    for (name, state) in &pipelines {
        assert!(!matches!(state, CachedPipelineState::Err(_)), "{name}: {state:?}");
    }
    for shader in ["pixel_perfect_composite", "darkness", "capsule_dim", "ripple", "vignette", "foreground_fog_mask"] {
        let path = format!("{shader}.wgsl");
        let compiled = pipelines.iter().any(|(name, state)| name.ends_with(&path) && matches!(state, CachedPipelineState::Ok(_)));
        assert!(compiled, "no {path} pipeline compiled");
    }
}
