use bevy_fluent::prelude::*;
use fluent_content::Content;
use unic_langid::LanguageIdentifier;

use crate::assets::LocaleAssets;
use crate::prelude::*;

pub struct LocalePlugin;

impl Plugin for LocalePlugin {
    fn build(&self, app: &mut App) {
        app.register_clicommand_args("locale", cli_locale);
        app.add_systems(
            OnExit(AppState::AssetsLoading),
            (detect_locales, init_l10n).chain(),
        );
        app.add_systems(
            Update,
            resolve_l10n
                .in_set(L10nResolveSet)
                .run_if(not(in_state(AppState::AssetsLoading))),
        );
    }
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct L10nResolveSet;

#[derive(Component)]
pub struct L10nKey(pub String);

#[derive(Resource)]
pub struct Locales(HashSet<LanguageIdentifier>);

fn cli_locale(In(args): In<Vec<String>>, mut locale: ResMut<Locale>, locales: Res<Locales>) {
    if args.len() != 1 {
        error!("\"locale <locale>\"");
        return;
    }
    match args[0].parse::<LanguageIdentifier>() {
        Ok(langid) => {
            if locales.0.contains(&langid) {
                locale.requested = langid;
            } else {
                error!("Unsupported locale: {:?}", args[0]);
            }
        },
        Err(e) => {
            error!("Invalid locale {:?}: {}", args[0], e);
        },
    }
}

fn detect_locales(world: &mut World) {
    let locales = {
        let assets = world.resource::<LocaleAssets>();
        let bundles = world.resource::<Assets<BundleAsset>>();
        assets
            .bundles
            .iter()
            .map(|handle| {
                let bundle = bundles.get(handle).unwrap();
                bundle.locales[0].clone()
            })
            .collect()
    };
    world.insert_resource(
        // FIXME: do actual locale selection
        Locale::new("en-US".parse().unwrap()).with_default("en-US".parse().unwrap()),
    );
    world.insert_resource(Locales(locales));
}

fn init_l10n(mut commands: Commands, l10n_builder: LocalizationBuilder, assets: Res<LocaleAssets>) {
    let l10n = l10n_builder.build(assets.bundles.iter());
    commands.insert_resource(l10n);
}

fn resolve_l10n(
    locale: Res<Locale>,
    l10n_builder: LocalizationBuilder,
    assets: Res<LocaleAssets>,
    mut ass_ev: EventReader<AssetEvent<BundleAsset>>,
    mut l10n: ResMut<Localization>,
    mut query: ParamSet<(
        Query<(&mut Text, &L10nKey), Changed<L10nKey>>,
        Query<(&mut Text, &L10nKey)>,
    )>,
) {
    let mut regenerate = false;

    if locale.is_changed() || !ass_ev.is_empty() {
        regenerate = true;
        ass_ev.clear();
    }

    if regenerate {
        *l10n = l10n_builder.build(assets.bundles.iter());
    }

    // closure for updating UI text
    let fn_update = |text: &mut Mut<Text>, key: &L10nKey| {
        if let Some(string) = l10n.content(&key.0) {
            text.sections[0].value = string;
        } else {
            text.sections[0].value = String::new();
        }
    };

    if regenerate {
        // query/update all if locale changed
        for (mut text, key) in &mut query.p1() {
            fn_update(&mut text, key);
        }
    } else {
        // only update any new/changed L10Keys otherwise
        for (mut text, key) in &mut query.p0() {
            fn_update(&mut text, key);
        }
    };
}
