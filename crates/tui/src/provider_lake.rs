//! Configured provider/model lake facade (#3830, Wave 5b).
//!
//! Single seam over the bundled Models.dev catalog, the configured-provider
//! predicate shared with `/provider`, and an optional live-catalog snapshot
//! (#3385 P1) that merges with bundled data. Pickers, hotbar route slots,
//! [`crate::model_inventory::ModelInventory`], slash completions, and subagent
//! validation should read model lists from here instead of the legacy hardcoded
//! table in [`crate::config::model_completion_names_for_provider`].

use std::sync::RwLock;

use codewhale_config::catalog::{CatalogOffering, CatalogSnapshot, bundled_catalog_offerings};

use crate::config::{
    ApiProvider, Config, model_completion_names_for_provider, provider_is_configured_for_active,
};

static BUNDLED_SNAPSHOT: std::sync::OnceLock<CatalogSnapshot> = std::sync::OnceLock::new();

/// Optional live-catalog snapshot, set by the app after a background refresh
/// (#3385 P1). When `None`, only bundled rows are visible.
static LIVE_SNAPSHOT: RwLock<Option<CatalogSnapshot>> = RwLock::new(None);

fn bundled_snapshot() -> &'static CatalogSnapshot {
    BUNDLED_SNAPSHOT.get_or_init(|| CatalogSnapshot {
        offerings: bundled_catalog_offerings(),
    })
}

/// Set the live-catalog snapshot. Call this after a background refresh
/// succeeds; the lake merges live rows over bundled rows on the next read.
/// Stale or empty snapshots are harmless — a `None` just means "bundled only."
pub fn set_live_snapshot(snapshot: CatalogSnapshot) {
    if let Ok(mut guard) = LIVE_SNAPSHOT.write() {
        *guard = Some(snapshot);
    }
}

/// Clear the live snapshot (e.g. on cache eviction or shutdown).
pub fn clear_live_snapshot() {
    if let Ok(mut guard) = LIVE_SNAPSHOT.write() {
        *guard = None;
    }
}

/// The merged catalog snapshot: live rows override bundled rows on
/// `(provider, wire_model_id)` identity. When no live snapshot is present,
/// this is just the bundled snapshot.
fn merged_snapshot() -> CatalogSnapshot {
    let live = LIVE_SNAPSHOT.read().ok().and_then(|guard| guard.clone());
    match live {
        None => bundled_snapshot().clone(),
        Some(live) => {
            use std::collections::BTreeMap;
            let mut merged: BTreeMap<(String, String), CatalogOffering> = BTreeMap::new();
            for row in &bundled_snapshot().offerings {
                merged.insert(
                    (row.provider.clone(), row.wire_model_id.clone()),
                    row.clone(),
                );
            }
            for row in &live.offerings {
                merged.insert(
                    (row.provider.clone(), row.wire_model_id.clone()),
                    row.clone(),
                );
            }
            CatalogSnapshot {
                offerings: merged.into_values().collect(),
            }
        }
    }
}

/// Maps an [`ApiProvider`] to its bundled-catalog provider id.
fn catalog_provider_id(provider: ApiProvider) -> &'static str {
    match provider {
        ApiProvider::DeepseekCN | ApiProvider::DeepseekAnthropic => "deepseek",
        ApiProvider::SiliconflowCn => "siliconflow",
        _ => provider.as_str(),
    }
}

fn push_unique_model(models: &mut Vec<String>, model: &str) {
    let model = model.trim();
    if model.is_empty() {
        return;
    }
    if !models
        .iter()
        .any(|existing| existing.eq_ignore_ascii_case(model))
    {
        models.push(model.to_string());
    }
}

fn catalog_models_from_offerings<'a>(
    offerings: impl IntoIterator<Item = &'a CatalogOffering>,
) -> Vec<String> {
    let mut rows: Vec<_> = offerings.into_iter().collect();
    rows.sort_by(|left, right| {
        right
            .default_for_provider
            .cmp(&left.default_for_provider)
            .then_with(|| left.wire_model_id.cmp(&right.wire_model_id))
    });
    let mut models = Vec::new();
    for row in rows {
        push_unique_model(&mut models, &row.wire_model_id);
    }
    models
}

/// Bundled-catalog model ids for one provider, merged with any live snapshot.
///
/// Returns provider wire ids from the merged catalog (bundled + live).
/// Providers not yet represented in the bundled asset fall back to the legacy
/// hardcoded table so routing surfaces stay usable until the asset catches up.
#[must_use]
pub fn all_catalog_models_for_provider(provider: ApiProvider) -> Vec<String> {
    let catalog_id = catalog_provider_id(provider);
    let merged = merged_snapshot();
    let mut models = catalog_models_from_offerings(merged.offerings_for_provider(catalog_id));
    if models.is_empty() {
        for model in model_completion_names_for_provider(provider) {
            push_unique_model(&mut models, model);
        }
    }
    models
}

/// Count of merged-catalog models for one provider (catalog view / dashboard).
#[must_use]
pub fn catalog_model_count_for_provider(provider: ApiProvider) -> usize {
    all_catalog_models_for_provider(provider).len()
}

/// Providers the user has set up — active provider, working credentials/OAuth,
/// or an explicit `[providers.<name>]` entry (#3830).
#[must_use]
pub fn configured_providers(config: &Config, active: ApiProvider) -> Vec<ApiProvider> {
    ApiProvider::sorted_for_display()
        .into_iter()
        .filter(|provider| provider_is_configured_for_active(config, *provider, active))
        .collect()
}

/// Catalog models for providers that qualify as configured for `active`.
#[must_use]
pub fn models_for_provider(
    config: &Config,
    active: ApiProvider,
    provider: ApiProvider,
) -> Vec<String> {
    if provider_is_configured_for_active(config, provider, active) {
        all_catalog_models_for_provider(provider)
    } else {
        Vec::new()
    }
}

/// Every built-in provider that carries at least one merged-catalog row.
#[must_use]
#[allow(dead_code)]
pub fn all_catalog_providers() -> Vec<ApiProvider> {
    let mut seen = Vec::new();
    for offering in &merged_snapshot().offerings {
        if let Some(provider) = ApiProvider::parse(&offering.provider)
            && !seen.contains(&provider)
        {
            seen.push(provider);
        }
    }
    seen
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{DEFAULT_TOGETHER_FLASH_MODEL, DEFAULT_TOGETHER_MODEL};

    #[test]
    fn together_catalog_includes_flash_from_bundled_asset() {
        let models = all_catalog_models_for_provider(ApiProvider::Together);
        assert!(
            models.contains(&DEFAULT_TOGETHER_MODEL.to_string()),
            "missing Together pro: {models:?}"
        );
        assert!(
            models.contains(&DEFAULT_TOGETHER_FLASH_MODEL.to_string()),
            "missing Together flash: {models:?}"
        );
    }

    #[test]
    fn configured_providers_matches_provider_predicate() {
        let config = Config::default();
        let active = ApiProvider::Deepseek;
        let expected: Vec<_> = ApiProvider::sorted_for_display()
            .into_iter()
            .filter(|provider| {
                crate::config::provider_is_configured_for_active(&config, *provider, active)
            })
            .collect();
        assert_eq!(configured_providers(&config, active), expected);
    }

    #[test]
    fn models_for_provider_filters_unconfigured_gateways() {
        let _env_lock = crate::test_support::lock_test_env();
        let _together = crate::test_support::EnvVarGuard::remove("TOGETHER_API_KEY");
        let config = Config::default();
        assert!(
            models_for_provider(&config, ApiProvider::Deepseek, ApiProvider::Together).is_empty()
        );
        assert!(
            !models_for_provider(&config, ApiProvider::Deepseek, ApiProvider::Deepseek).is_empty()
        );
    }

    #[test]
    fn live_snapshot_merges_over_bundled() {
        clear_live_snapshot();
        // With no live snapshot, we get bundled models.
        let bundled = all_catalog_models_for_provider(ApiProvider::Deepseek);
        assert!(!bundled.is_empty());

        // Set a live snapshot that adds a synthetic model.
        let live = CatalogSnapshot {
            offerings: vec![CatalogOffering {
                provider: "deepseek".to_string(),
                wire_model_id: "deepseek-v4-synthetic".to_string(),
                endpoint_key: "chat".to_string(),
                ..Default::default()
            }],
        };
        set_live_snapshot(live);
        let merged = all_catalog_models_for_provider(ApiProvider::Deepseek);
        assert!(merged.contains(&"deepseek-v4-synthetic".to_string()));
        // The bundled model is still present.
        assert!(merged.iter().any(|m| bundled.contains(m)));

        clear_live_snapshot();
        let after_clear = all_catalog_models_for_provider(ApiProvider::Deepseek);
        assert_eq!(after_clear, bundled);
    }
}
