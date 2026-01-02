use anyhow::Result;
use serde::{Deserialize, Serialize};
use crate::supabase;

/// Type de configuration pour évolution future
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ConfigType {
    Streaming,  // Config actuelle pour média streaming
    Storage,    // Config future pour stockage NAS
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MasterConfig {
    pub id: String,
    #[serde(default)]
    pub config_type: Option<String>,  // "streaming" ou "storage" pour évolution future
    pub radarr_config: Option<serde_json::Value>,
    pub sonarr_config: Option<serde_json::Value>,
    pub prowlarr_config: Option<serde_json::Value>,
    pub bazarr_config: Option<serde_json::Value>,
    pub jellyfin_config: Option<serde_json::Value>,
    pub jellyseerr_config: Option<serde_json::Value>,
    pub decypharr_config: Option<serde_json::Value>,
}

/// Récupère la master_config depuis Supabase
///
/// IMPORTANT: Fetch dynamique à chaque installation - ne jamais hardcoder!
///
/// # Arguments
/// * `config_type` - Optionnel: "streaming" ou "storage" pour filtrer par type
pub async fn fetch_master_config(config_type: Option<&str>) -> Result<Option<MasterConfig>> {
    let client = reqwest::Client::new();
    let supabase_url = supabase::get_supabase_url_public();
    let service_key = supabase::get_supabase_service_key();

    println!("[MasterConfig] 🔄 Fetching master_config from Supabase (type: {:?})...", config_type);

    // Construire la query avec filtres
    let mut query_params = vec![
        ("select", "*"),
        ("is_active", "eq.true"),
        ("order", "created_at.desc"),
        ("limit", "1"),
    ];

    // Si un type est spécifié, filtrer dessus
    // Pour l'instant on récupère juste la première active
    // TODO future: ajouter filter sur config_type quand la colonne sera ajoutée

    let response = client
        .get(format!("{}/rest/v1/master_configs", supabase_url))
        .query(&query_params)
        .header("apikey", &service_key)
        .header("Authorization", format!("Bearer {}", service_key))
        .send()
        .await?;

    if !response.status().is_success() {
        println!("[MasterConfig] ⚠️  Failed to fetch master_config: {}", response.status());
        return Ok(None);
    }

    let configs: Vec<MasterConfig> = response.json().await?;

    if let Some(config) = configs.first() {
        println!("[MasterConfig] ✅ Loaded master_config: {} (type: {:?})",
                 config.id, config.config_type);
        Ok(Some(config.clone()))
    } else {
        println!("[MasterConfig] ⚠️  No active master_config found");
        Ok(None)
    }
}
