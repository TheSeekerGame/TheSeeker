use crate::prelude::*;

mod console;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugin(iyes_ui::UiExtrasPlugin);
        app.add_plugin(self::console::UiConsolePlugin);
    }
}
