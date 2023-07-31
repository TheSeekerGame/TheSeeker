use crate::prelude::*;

mod console;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(iyes_ui::UiExtrasPlugin);
        app.add_plugins(self::console::UiConsolePlugin);
    }
}
