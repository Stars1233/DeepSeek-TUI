use crate::commands::CommandResult;
use crate::commands::traits::{
    Command, CommandGroup, CommandInfo, FunctionCommand, RegisterCommand,
};
use crate::localization::MessageId;
use crate::plugins;
use crate::tui::app::App;

pub struct PluginsCommands;

impl CommandGroup for PluginsCommands {
    fn commands(&self) -> Vec<Box<dyn Command>> {
        vec![
            Box::new(FunctionCommand::new(
                PluginListCmd::info(),
                PluginListCmd::execute,
            )),
            Box::new(FunctionCommand::new(
                PluginEnableCmd::info(),
                PluginEnableCmd::execute,
            )),
            Box::new(FunctionCommand::new(
                PluginDisableCmd::info(),
                PluginDisableCmd::execute,
            )),
            Box::new(FunctionCommand::new(
                PluginInfoCmd::info(),
                PluginInfoCmd::execute,
            )),
        ]
    }
}

pub(in crate::commands) const PLUGIN_LIST_INFO: CommandInfo = CommandInfo {
    name: "plugin",
    aliases: &["plugins"],
    usage: "/plugin list",
    description_id: MessageId::CmdPluginDescription,
};

pub(in crate::commands) struct PluginListCmd;

impl RegisterCommand for PluginListCmd {
    fn info() -> &'static CommandInfo {
        &PLUGIN_LIST_INFO
    }

    fn execute(app: &mut App, arg: Option<&str>) -> CommandResult {
        if let Some(arg) = arg {
            if arg.starts_with("list") {
                plugin_list(app)
            } else if arg.starts_with("enable ") {
                let name = arg.strip_prefix("enable ").unwrap_or("").trim();
                plugin_enable(app, name)
            } else if arg.starts_with("disable ") {
                let name = arg.strip_prefix("disable ").unwrap_or("").trim();
                plugin_disable(app, name)
            } else {
                plugin_info(app, arg.trim())
            }
        } else {
            plugin_list(app)
        }
    }
}

pub(in crate::commands) const PLUGIN_ENABLE_INFO: CommandInfo = CommandInfo {
    name: "plugin enable",
    aliases: &[],
    usage: "/plugin enable <name>",
    description_id: MessageId::CmdPluginDescription,
};

pub(in crate::commands) struct PluginEnableCmd;

impl RegisterCommand for PluginEnableCmd {
    fn info() -> &'static CommandInfo {
        &PLUGIN_ENABLE_INFO
    }

    fn execute(app: &mut App, arg: Option<&str>) -> CommandResult {
        let name = arg.unwrap_or("").trim();
        if name.is_empty() {
            CommandResult::error("Usage: /plugin enable <name>")
        } else {
            plugin_enable(app, name)
        }
    }
}

pub(in crate::commands) const PLUGIN_DISABLE_INFO: CommandInfo = CommandInfo {
    name: "plugin disable",
    aliases: &[],
    usage: "/plugin disable <name>",
    description_id: MessageId::CmdPluginDescription,
};

pub(in crate::commands) struct PluginDisableCmd;

impl RegisterCommand for PluginDisableCmd {
    fn info() -> &'static CommandInfo {
        &PLUGIN_DISABLE_INFO
    }

    fn execute(app: &mut App, arg: Option<&str>) -> CommandResult {
        let name = arg.unwrap_or("").trim();
        if name.is_empty() {
            CommandResult::error("Usage: /plugin disable <name>")
        } else {
            plugin_disable(app, name)
        }
    }
}

pub(in crate::commands) const PLUGIN_INFO_INFO: CommandInfo = CommandInfo {
    name: "plugin info",
    aliases: &[],
    usage: "/plugin info <name>",
    description_id: MessageId::CmdPluginDescription,
};

pub(in crate::commands) struct PluginInfoCmd;

impl RegisterCommand for PluginInfoCmd {
    fn info() -> &'static CommandInfo {
        &PLUGIN_INFO_INFO
    }

    fn execute(app: &mut App, arg: Option<&str>) -> CommandResult {
        let name = arg.unwrap_or("").trim();
        if name.is_empty() {
            CommandResult::error("Usage: /plugin info <name>")
        } else {
            plugin_info(app, name)
        }
    }
}

fn plugin_list(app: &App) -> CommandResult {
    let plugins = plugins::try_with_registry(|r| r.list()).unwrap_or_default();

    if plugins.is_empty() {
        return CommandResult::message("No plugins discovered.");
    }

    let mut out = String::new();
    out.push_str(&format!("Plugins ({})\n", plugins.len()));
    out.push_str(&"=".repeat(40));
    out.push('\n');

    for (name, plugin) in plugins {
        let status = if plugin.enabled {
            "enabled"
        } else {
            "disabled"
        };
        let description = plugin
            .manifest
            .plugin
            .description
            .as_deref()
            .unwrap_or("No description");
        out.push_str(&format!("• {} [{}]\n  {}\n", name, status, description));
    }

    CommandResult::message(out)
}

fn plugin_enable(_app: &App, name: &str) -> CommandResult {
    let result = plugins::with_registry(|r| r.enable(name));

    match result {
        Some(true) => CommandResult::message(format!("Plugin '{}' enabled.", name)),
        Some(false) => CommandResult::error(format!("Plugin '{}' not found.", name)),
        None => CommandResult::error("Plugin registry not initialized."),
    }
}

fn plugin_disable(_app: &App, name: &str) -> CommandResult {
    let result = plugins::with_registry(|r| r.disable(name));

    match result {
        Some(true) => CommandResult::message(format!("Plugin '{}' disabled.", name)),
        Some(false) => CommandResult::error(format!("Plugin '{}' not found.", name)),
        None => CommandResult::error("Plugin registry not initialized."),
    }
}

fn plugin_info(_app: &App, name: &str) -> CommandResult {
    let plugin = plugins::try_with_registry(|r| r.get(name));

    match plugin {
        Some(Some(plugin)) => {
            let mut out = String::new();
            out.push_str(&format!("{}\n", plugin.manifest.plugin.name));
            out.push_str(&"=".repeat(40));
            out.push('\n');
            if let Some(desc) = &plugin.manifest.plugin.description {
                out.push_str(&format!("Description: {}\n", desc));
            }
            if let Some(version) = &plugin.manifest.plugin.version {
                out.push_str(&format!("Version: {}\n", version));
            }
            if let Some(author) = &plugin.manifest.plugin.author {
                out.push_str(&format!("Author: {}\n", author));
            }
            out.push_str(&format!(
                "Status: {}\n",
                if plugin.enabled {
                    "enabled"
                } else {
                    "disabled"
                }
            ));
            out.push_str(&format!("Path: {}\n", plugin.base_path.display()));
            CommandResult::message(out)
        }
        Some(None) => CommandResult::error(format!("Plugin '{}' not found.", name)),
        None => CommandResult::error("Plugin registry not initialized."),
    }
}
