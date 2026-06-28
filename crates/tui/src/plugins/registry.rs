use std::collections::HashMap;

use super::manifest::LoadedPlugin;

#[derive(Debug, Clone)]
pub struct PluginRegistry {
    plugins: HashMap<String, LoadedPlugin>,
    user_overrides: HashMap<String, bool>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
            user_overrides: HashMap::new(),
        }
    }

    pub fn register(&mut self, name: String, plugin: LoadedPlugin) {
        self.plugins.insert(name, plugin);
    }

    pub fn enable(&mut self, name: &str) -> bool {
        if let Some(plugin) = self.plugins.get_mut(name) {
            plugin.enabled = true;
            self.user_overrides.insert(name.to_string(), true);
            true
        } else {
            false
        }
    }

    pub fn disable(&mut self, name: &str) -> bool {
        if let Some(plugin) = self.plugins.get_mut(name) {
            plugin.enabled = false;
            self.user_overrides.insert(name.to_string(), false);
            true
        } else {
            false
        }
    }

    pub fn list(&self) -> Vec<(&String, &LoadedPlugin)> {
        self.plugins.iter().collect()
    }

    pub fn get(&self, name: &str) -> Option<&LoadedPlugin> {
        self.plugins.get(name)
    }

    pub fn is_enabled(&self, name: &str) -> bool {
        self.plugins.get(name).map_or(false, |p| p.enabled)
    }

    pub fn len(&self) -> usize {
        self.plugins.len()
    }

    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }
}
