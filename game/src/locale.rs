use bevy::asset::{LoadedFolder, LoadState};
use bevy_fluent::prelude::*;
use fluent_content::Content;
use unic_langid::LanguageIdentifier;

use crate::prelude::*;

pub struct LocalePlugin;

impl Plugin for LocalePlugin {
    fn build(&self, app: &mut App) {
        app.register_clicommand_args("locale", cli_locale);
        app.insert_resource(
            Locale::new("en-US".parse().unwrap()).with_default("en-US".parse().unwrap()),
        );
        app.add_systems(Update,
            init_l10n
                .track_progress()
                .run_if(in_state(AppState::AssetsLoading))
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

#[derive(Resource)]
pub struct LocalesFolder(Handle<LoadedFolder>);

fn init_l10n(
    mut commands: Commands,
    l10n_builder: LocalizationBuilder,
    l10n_bundles: Res<Assets<BundleAsset>>,
    folder: Option<Res<LocalesFolder>>,
    mut done: Local<bool>,
    ass: Res<AssetServer>,
) -> Progress {
    match (*done, folder) {
        (false, None) => {
            commands.insert_resource(LocalesFolder(ass.load_folder("locale")));
        }
        (false, Some(folder)) => {
            if let Some(LoadState::Loaded) = ass.get_load_state(&folder.0) {
                let locales = Locales(l10n_bundles.iter()
                    .map(|(_, bundle)| bundle.locales[0].clone())
                    .collect());
                let l10n = l10n_builder.build(&folder.0);
                commands.insert_resource(locales);
                commands.insert_resource(l10n);
                *done = true;
            }
        }
        (true, _) => {}
    }
    (*done).into()
}

fn resolve_l10n(
    locale: Res<Locale>,
    l10n_builder: LocalizationBuilder,
    mut ass_ev: EventReader<AssetEvent<BundleAsset>>,
    mut l10n: ResMut<Localization>,
    l10n_folder: Res<LocalesFolder>,
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
        *l10n = l10n_builder.build(&l10n_folder.0);
    }

    // closure for updating UI text
    let fn_update = |text: &mut Mut<Text>, key: &L10nKey| {
        if let Some(string) = l10n.content(&key.0) {
            text.sections[0].value = string;
        } else {
            text.sections[0].value = key.0.clone();
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
