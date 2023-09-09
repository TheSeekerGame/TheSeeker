use crate::prelude::*;

pub struct CliPlugin;

impl Plugin for CliPlugin {
    fn build(&self, app: &mut App) {
        app.register_clicommand_noargs("exit", cli_exit);
        app.register_clicommand_noargs("help", cli_help);
    }
}

fn cli_exit(mut evw_exit: EventWriter<bevy::app::AppExit>) {
    evw_exit.send(bevy::app::AppExit);
}

fn cli_help(clicommands: Res<iyes_cli::CliCommands>) {
    let mut aggregate: HashMap<&str, (bool, bool)> = HashMap::with_capacity(clicommands.commands_noargs.capacity() + clicommands.commands_args.capacity());

    for noargscmd in clicommands.commands_noargs.keys() {
        if let Some(existing) = aggregate.get_mut(noargscmd.as_str()) {
            existing.0 = true;
        } else {
            aggregate.insert(noargscmd.as_str(), (true, false));
        }
    }

    for argscmd in clicommands.commands_args.keys() {
        if let Some(existing) = aggregate.get_mut(argscmd.as_str()) {
            existing.1 = true;
        } else {
            aggregate.insert(argscmd.as_str(), (false, true));
        }
    }

    let mut sorted: Vec<_> = aggregate.into_iter().collect();
    sorted.sort_unstable_by_key(|x| x.0);

    info!("List of registered CliCommands:");
    for (name, (is_noargs, is_args)) in sorted {
        info!(
            "{}{}{}",
            name,
            if is_noargs {
                " (noargs)"
            } else {
                ""
            },
            if is_args {
                " (args)"
            } else {
                ""
            }
        );
    }
}
