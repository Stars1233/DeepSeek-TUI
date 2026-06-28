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

fn plugin_list(_app: &App) -> CommandResult {
    plugins::try_with_registry(|r| {
        if r.is_empty() {
            return CommandResult::message("No plugins discovered.");
        }

        let mut out = String::new();
        out.push_str(&format!("Plugins ({})\n", r.len()));
        out.push_str(&"=".repeat(40));
        out.push('\n');

        for (name, plugin) in r.list() {
            let status = if r.is_enabled(name) {
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
    })
    .unwrap_or_else(|| CommandResult::error("Plugin registry not initialized."))
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
    plugins::try_with_registry(|r| match r.get(name) {
        Some(plugin) => {
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
            if let Some(skills) = &plugin.manifest.skills {
                if let Some(path) = &skills.path {
                    out.push_str(&format!("Skills: {}\n", path));
                }
            }
            if let Some(mcp_servers) = &plugin.manifest.mcp_servers {
                out.push_str(&format!("MCP servers: {}\n", mcp_servers.len()));
                for (server_name, server) in mcp_servers {
                    out.push_str(&format!("  - {}: {}\n", server_name, server.command));
                    if let Some(args) = &server.args {
                        out.push_str(&format!("    args: {}\n", args.join(" ")));
                    }
                    if let Some(env) = &server.env {
                        out.push_str(&format!("    env vars: {}\n", env.len()));
                    }
                    if let Some(cwd) = &server.cwd {
                        out.push_str(&format!("    cwd: {}\n", cwd));
                    }
                    if let Some(sandbox) = server.sandbox {
                        out.push_str(&format!("    sandbox: {}\n", sandbox));
                    }
                }
            }
            CommandResult::message(out)
        }
        None => CommandResult::error(format!("Plugin '{}' not found.", name)),
    })
    .unwrap_or_else(|| CommandResult::error("Plugin registry not initialized."))
}
