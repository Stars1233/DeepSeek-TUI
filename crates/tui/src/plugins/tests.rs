use std::path::PathBuf;

use super::manifest::{PluginManifest, PluginMeta};
use super::registry::PluginRegistry;

#[test]
fn test_manifest_parsing() {
    let toml_content = r#"
[plugin]
name = "test-plugin"
description = "A test plugin"
version = "1.0.0"
author = "Test Author"

[when]
os = ["windows", "linux"]
binaries = ["cargo"]
"#;

    let manifest: PluginManifest = toml::from_str(toml_content).unwrap();
    assert_eq!(manifest.plugin.name, "test-plugin");
    assert_eq!(manifest.plugin.description.unwrap(), "A test plugin");
    assert_eq!(manifest.plugin.version.unwrap(), "1.0.0");
    assert_eq!(manifest.plugin.author.unwrap(), "Test Author");
}

#[test]
fn test_manifest_when_os_filter() {
    let manifest = PluginManifest {
        plugin: PluginMeta {
            name: "test".to_string(),
            description: None,
            version: None,
            author: None,
        },
        skills: None,
        mcp_servers: None,
        when: Some(super::manifest::PluginWhen {
            os: Some(vec![std::env::consts::OS.to_string()]),
            binaries: None,
        }),
    };

    assert!(manifest.check_when());
}

#[test]
fn test_manifest_when_os_mismatch() {
    let manifest = PluginManifest {
        plugin: PluginMeta {
            name: "test".to_string(),
            description: None,
            version: None,
            author: None,
        },
        skills: None,
        mcp_servers: None,
        when: Some(super::manifest::PluginWhen {
            os: Some(vec!["nonexistent-os".to_string()]),
            binaries: None,
        }),
    };

    assert!(!manifest.check_when());
}

#[test]
fn test_registry_enable_disable() {
    let mut registry = PluginRegistry::new();

    let manifest = PluginManifest {
        plugin: PluginMeta {
            name: "test-plugin".to_string(),
            description: None,
            version: None,
            author: None,
        },
        skills: None,
        mcp_servers: None,
        when: None,
    };

    let plugin = super::manifest::LoadedPlugin {
        manifest,
        base_path: PathBuf::new(),
        enabled: false,
    };

    registry.register("test-plugin".to_string(), plugin);

    assert!(!registry.is_enabled("test-plugin"));
    assert!(registry.enable("test-plugin"));
    assert!(registry.is_enabled("test-plugin"));
    assert!(registry.disable("test-plugin"));
    assert!(!registry.is_enabled("test-plugin"));
}

#[test]
fn test_registry_list() {
    let mut registry = PluginRegistry::new();

    let manifest1 = PluginManifest {
        plugin: PluginMeta {
            name: "plugin-1".to_string(),
            description: None,
            version: None,
            author: None,
        },
        skills: None,
        mcp_servers: None,
        when: None,
    };

    let manifest2 = PluginManifest {
        plugin: PluginMeta {
            name: "plugin-2".to_string(),
            description: None,
            version: None,
            author: None,
        },
        skills: None,
        mcp_servers: None,
        when: None,
    };

    let plugin1 = super::manifest::LoadedPlugin {
        manifest: manifest1,
        base_path: PathBuf::new(),
        enabled: true,
    };

    let plugin2 = super::manifest::LoadedPlugin {
        manifest: manifest2,
        base_path: PathBuf::new(),
        enabled: false,
    };

    registry.register("plugin-1".to_string(), plugin1);
    registry.register("plugin-2".to_string(), plugin2);

    assert_eq!(registry.len(), 2);
    assert!(registry.is_enabled("plugin-1"));
    assert!(!registry.is_enabled("plugin-2"));
}
