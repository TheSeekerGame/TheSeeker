use crate::prelude::*;

pub struct CliPlugin;

impl Plugin for CliPlugin {
    fn build(&self, app: &mut App) {
        app.register_clicommand_noargs("exit", exit);
        app.register_clicommand_args("spawn_script", spawn_script);
    }
}

fn exit(mut evw_exit: EventWriter<bevy::app::AppExit>) {
    evw_exit.send(bevy::app::AppExit);
}

fn spawn_script(In(args): In<Vec<String>>, world: &mut World) {
    use theseeker_engine::assets::script::Script;

    if args.len() != 1 {
        error!("\"spawn_script <script_asset_key>\"");
        return;
    }

    world.spawn((AssetKey::<Script>::new(&args[0]),));
}
