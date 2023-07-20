use crate::prelude::*;

pub struct CliPlugin;

impl Plugin for CliPlugin {
    fn build(&self, app: &mut App) {
        app.register_clicommand_noargs("exit", exit);
    }
}

fn exit(mut evw_exit: EventWriter<bevy::app::AppExit>) {
    evw_exit.send(bevy::app::AppExit);
}
