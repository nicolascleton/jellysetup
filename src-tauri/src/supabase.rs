use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashSet;
use std::sync::Mutex;
use once_cell::sync::Lazy;

// Set des schémas déjà initialisés (un par Pi)
static INITIALIZED_SCHEMAS: Lazy<Mutex<HashSet<String>>> = Lazy::new(|| Mutex::new(HashSet::new()));

// Ces valeurs sont injectées au build via .env
fn get_supabase_url() -> String {
    option_env!("SUPABASE_URL")
        .unwrap_or("https://ncxowprkehliisvnpmlt.supabase.co")
        .to_string()
}

fn get_supabase_key() -> String {
    option_env!("SUPABASE_ANON_KEY")
        .unwrap_or("your-anon-key")
        .to_string()
}

/// Get service key for Supabazarr (allows write access)
pub fn get_supabase_service_key() -> String {
    option_env!("SUPABASE_SERVICE_KEY")
        .unwrap_or("your-service-key")
        .to_string()
}

/// Get Supabase URL for external use
pub fn get_supabase_url_public() -> String {
    get_supabase_url()
}

/// Get ANON key (public, safe for client-side use)
/// SÉCURITÉ: Cette clé est publique et peut être exposée dans l'app
pub fn get_supabase_anon_key() -> String {
    get_supabase_key()
}

/// Convertit le nom du Pi en nom de schéma PostgreSQL valide
fn pi_name_to_schema(pi_name: &str) -> String {
    pi_name.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' })
        .collect()
}

#[derive(Debug, Serialize, Deserialize)]
struct ConfigRow {
    id: Option<String>,
    pi_name: Option<String>,
    local_ip: Option<String>,
    ssh_public_key: Option<String>,
    ssh_private_key_encrypted: Option<String>,
    ssh_host_fingerprint: Option<String>,  // Fingerprint du serveur SSH du Pi
    status: Option<String>,
    installer_version: Option<String>,
}

#[derive(Debug, Deserialize)]
struct InitResponse {
    #[serde(default)]
    success: bool,
    message: Option<String>,
    schema: Option<String>,
    tables: Option<Vec<String>>,
    error: Option<String>,
}

/// Initialise le schéma Supabase pour un Pi spécifique
pub async fn ensure_schema_initialized(pi_name: &str) -> Result<String> {
    let schema_name = pi_name_to_schema(pi_name);

    // Skip si déjà initialisé
    {
        let schemas = INITIALIZED_SCHEMAS.lock().unwrap();
        if schemas.contains(&schema_name) {
            return Ok(schema_name);
        }
    }

    let client = reqwest::Client::new();
    let supabase_url = get_supabase_url();
    let service_key = get_supabase_service_key();

    println!("[Supabase] Initializing schema '{}' for Pi '{}'...", schema_name, pi_name);

    let response = client
        .post(format!("{}/functions/v1/jellysetup-init", supabase_url))
        .header("Authorization", format!("Bearer {}", service_key))
        .header("Content-Type", "application/json")
        .json(&json!({ "pi_name": pi_name }))
        .send()
        .await;

    // Gérer les erreurs Supabase sans bloquer l'installation
    let result = match response {
        Ok(resp) => {
            match resp.json::<InitResponse>().await {
                Ok(r) => Some(r),
                Err(e) => {
                    println!("[Supabase] Warning: could not parse response: {}", e);
                    None
                }
            }
        }
        Err(e) => {
            println!("[Supabase] Warning: request failed: {}", e);
            None
        }
    };

    if result.as_ref().map(|r| r.success).unwrap_or(false) {
        println!("[Supabase] Schema '{}' initialized: {:?}",
                 result.as_ref().and_then(|r| r.schema.clone()).unwrap_or_default(),
                 result.as_ref().and_then(|r| r.tables.clone()));
        let mut schemas = INITIALIZED_SCHEMAS.lock().unwrap();
        schemas.insert(schema_name.clone());
        Ok(schema_name)
    } else {
        println!("[Supabase] Schema init warning: {:?}", result.as_ref().and_then(|r| r.error.clone()));
        // On continue quand même, le schéma existe peut-être déjà
        let mut schemas = INITIALIZED_SCHEMAS.lock().unwrap();
        schemas.insert(schema_name.clone());
        Ok(schema_name)
    }
}

/// Sauvegarde une installation dans le schéma dédié au Pi
/// Note: ssh_public_key et ssh_private_key_encrypted sont optionnels pour les installations par mot de passe
pub async fn save_installation(
    pi_name: &str,
    pi_ip: &str,
    ssh_public_key: Option<&str>,
    ssh_private_key_encrypted: Option<&str>,
    ssh_host_fingerprint: Option<&str>,
    installer_version: &str,
) -> Result<String> {
    // S'assurer que le schéma existe et récupérer son nom
    let schema_name = ensure_schema_initialized(pi_name).await?;

    let client = reqwest::Client::new();
    let supabase_url = get_supabase_url();
    let service_key = get_supabase_service_key();

    // Vérifier si une config existe déjà dans ce schéma
    let existing = check_existing_config(&schema_name).await?;

    if let Some(id) = existing {
        // Mettre à jour la config existante (reflash de carte SD = nouveau fingerprint)
        let mut body = json!({
            "local_ip": pi_ip,
            "status": "updating",
            "installer_version": installer_version,
            "last_seen": chrono::Utc::now().to_rfc3339()
        });

        // Ajouter les clés SSH si fournies (auth par clé)
        if let Some(pub_key) = ssh_public_key {
            body["ssh_public_key"] = json!(pub_key);
        }
        if let Some(priv_key) = ssh_private_key_encrypted {
            body["ssh_private_key_encrypted"] = json!(priv_key);
        }

        // Toujours mettre à jour le fingerprint si fourni (important lors d'un reflash)
        if let Some(fp) = ssh_host_fingerprint {
            body["ssh_host_fingerprint"] = json!(fp);
            println!("[Supabase] Updating host fingerprint (SD card reflash detected)");
        }

        client
            .patch(format!(
                "{}/rest/v1/config?id=eq.{}",
                supabase_url, id
            ))
            .header("apikey", &service_key)
            .header("Authorization", format!("Bearer {}", service_key))
            .header("Content-Type", "application/json")
            .header("Content-Profile", &schema_name)
            .json(&body)
            .send()
            .await?;

        println!("[Supabase] Updated config in schema '{}': {}", schema_name, id);
        return Ok(id);
    }

    // Mettre à jour la config créée automatiquement par l'init
    let mut body = json!({
        "local_ip": pi_ip,
        "status": "pending",
        "installer_version": installer_version
    });

    // Ajouter les clés SSH si fournies (auth par clé)
    if let Some(pub_key) = ssh_public_key {
        body["ssh_public_key"] = json!(pub_key);
    }
    if let Some(priv_key) = ssh_private_key_encrypted {
        body["ssh_private_key_encrypted"] = json!(priv_key);
    }
    if let Some(fp) = ssh_host_fingerprint {
        body["ssh_host_fingerprint"] = json!(fp);
    }

    // Récupérer l'ID de la config existante (créée par l'init)
    let response = client
        .get(format!("{}/rest/v1/config?select=id&limit=1", supabase_url))
        .header("apikey", &service_key)
        .header("Authorization", format!("Bearer {}", service_key))
        .header("Accept-Profile", &schema_name)
        .send()
        .await;

    // Gérer les erreurs Supabase sans bloquer
    let configs: Vec<ConfigRow> = match response {
        Ok(resp) => resp.json().await.unwrap_or_default(),
        Err(e) => {
            println!("[Supabase] Warning: could not fetch config: {}", e);
            vec![]
        }
    };

    if let Some(config) = configs.first() {
        if let Some(id) = &config.id {
            // Mettre à jour cette config
            client
                .patch(format!(
                    "{}/rest/v1/config?id=eq.{}",
                    supabase_url, id
                ))
                .header("apikey", &service_key)
                .header("Authorization", format!("Bearer {}", service_key))
                .header("Content-Type", "application/json")
                .header("Content-Profile", &schema_name)
                .json(&body)
                .send()
                .await?;

            println!("[Supabase] Updated config in schema '{}': {}", schema_name, id);
            return Ok(id.clone());
        }
    }

    // Si pas de config, en créer une (ne devrait pas arriver)
    let response = client
        .post(format!("{}/rest/v1/config", supabase_url))
        .header("apikey", &service_key)
        .header("Authorization", format!("Bearer {}", service_key))
        .header("Content-Type", "application/json")
        .header("Content-Profile", &schema_name)
        .header("Prefer", "return=representation")
        .json(&body)
        .send()
        .await?;

    let status = response.status();
    let text = response.text().await?;

    if !status.is_success() {
        println!("[Supabase] Warning creating config: {} - {}", status, text);
        // Ne pas bloquer l'installation pour une erreur Supabase
        return Ok("local".to_string());
    }

    let result: Vec<ConfigRow> = serde_json::from_str(&text).unwrap_or_default();
    let id = result.first().and_then(|i| i.id.clone()).unwrap_or_else(|| "local".to_string());

    println!("[Supabase] Created config in schema '{}': {}", schema_name, id);
    Ok(id)
}

/// Met à jour le statut d'une installation
pub async fn update_status(pi_name: &str, config_id: &str, status: &str, error: Option<&str>) -> Result<()> {
    let schema_name = pi_name_to_schema(pi_name);
    let client = reqwest::Client::new();
    let supabase_url = get_supabase_url();
    let service_key = get_supabase_service_key();

    let mut body = json!({
        "status": status,
        "last_seen": chrono::Utc::now().to_rfc3339()
    });

    if let Some(err) = error {
        body["error_message"] = json!(err);
    }

    client
        .patch(format!(
            "{}/rest/v1/config?id=eq.{}",
            supabase_url, config_id
        ))
        .header("apikey", &service_key)
        .header("Authorization", format!("Bearer {}", service_key))
        .header("Content-Type", "application/json")
        .header("Content-Profile", &schema_name)
        .json(&body)
        .send()
        .await?;

    Ok(())
}

/// Ajoute un log d'installation dans le schéma du Pi
pub async fn add_log(
    pi_name: &str,
    step: &str,
    status: &str,
    message: &str,
    duration_ms: Option<i64>,
) -> Result<()> {
    let schema_name = pi_name_to_schema(pi_name);
    let client = reqwest::Client::new();
    let supabase_url = get_supabase_url();
    let service_key = get_supabase_service_key();

    let body = json!({
        "step": step,
        "status": status,
        "message": message,
        "duration_ms": duration_ms
    });

    client
        .post(format!("{}/rest/v1/logs", supabase_url))
        .header("apikey", &service_key)
        .header("Authorization", format!("Bearer {}", service_key))
        .header("Content-Type", "application/json")
        .header("Content-Profile", &schema_name)
        .json(&body)
        .send()
        .await?;

    Ok(())
}

/// Vérifie si une config existe déjà dans le schéma
async fn check_existing_config(schema_name: &str) -> Result<Option<String>> {
    let client = reqwest::Client::new();
    let supabase_url = get_supabase_url();
    let service_key = get_supabase_service_key();

    let response = client
        .get(format!(
            "{}/rest/v1/config?select=id,status&limit=1",
            supabase_url
        ))
        .header("apikey", &service_key)
        .header("Authorization", format!("Bearer {}", service_key))
        .header("Accept-Profile", schema_name)
        .send()
        .await?;

    let status = response.status();
    let text = response.text().await?;

    if !status.is_success() {
        println!("[Supabase] check_existing_config error ({}): {}", status, text);
        return Ok(None);
    }

    // Parse as array - handle both empty array and actual results
    match serde_json::from_str::<Vec<ConfigRow>>(&text) {
        Ok(results) => Ok(results.first().and_then(|i| i.id.clone())),
        Err(e) => {
            println!("[Supabase] check_existing_config parse error: {} - response: {}", e, text);
            Ok(None)
        }
    }
}

/// Sauvegarde la configuration du Pi (credentials, services, etc.)
pub async fn save_pi_config(
    pi_name: &str,
    config_id: &str,
    alldebrid_key: Option<&str>,
    ygg_passkey: Option<&str>,
    cloudflare_token: Option<&str>,
    services_config: Option<serde_json::Value>,
) -> Result<()> {
    let schema_name = pi_name_to_schema(pi_name);
    let client = reqwest::Client::new();
    let supabase_url = get_supabase_url();
    let service_key = get_supabase_service_key();

    let mut body = json!({
        "last_seen": chrono::Utc::now().to_rfc3339()
    });

    if let Some(key) = alldebrid_key {
        body["alldebrid_api_key"] = json!(key);
    }
    if let Some(key) = ygg_passkey {
        body["ygg_passkey"] = json!(key);
    }
    if let Some(token) = cloudflare_token {
        body["cloudflare_token"] = json!(token);
    }
    if let Some(config) = services_config {
        body["services_config"] = config;
    }

    client
        .patch(format!(
            "{}/rest/v1/config?id=eq.{}",
            supabase_url, config_id
        ))
        .header("apikey", &service_key)
        .header("Authorization", format!("Bearer {}", service_key))
        .header("Content-Type", "application/json")
        .header("Content-Profile", &schema_name)
        .json(&body)
        .send()
        .await?;

    Ok(())
}

/// Enregistre un service Docker dans le schéma du Pi
pub async fn save_service(
    pi_name: &str,
    service_name: &str,
    container_id: Option<&str>,
    status: &str,
    port: Option<i32>,
    image: Option<&str>,
    config: Option<serde_json::Value>,
) -> Result<()> {
    let schema_name = pi_name_to_schema(pi_name);
    let client = reqwest::Client::new();
    let supabase_url = get_supabase_url();
    let service_key = get_supabase_service_key();

    let mut body = json!({
        "service_name": service_name,
        "status": status,
        "last_health_check": chrono::Utc::now().to_rfc3339()
    });

    if let Some(id) = container_id {
        body["container_id"] = json!(id);
    }
    if let Some(p) = port {
        body["port"] = json!(p);
    }
    if let Some(img) = image {
        body["image"] = json!(img);
    }
    if let Some(cfg) = config {
        body["config"] = cfg;
    }

    // Upsert basé sur service_name
    client
        .post(format!("{}/rest/v1/services", supabase_url))
        .header("apikey", &service_key)
        .header("Authorization", format!("Bearer {}", service_key))
        .header("Content-Type", "application/json")
        .header("Content-Profile", &schema_name)
        .header("Prefer", "resolution=merge-duplicates")
        .json(&body)
        .send()
        .await?;

    Ok(())
}

/// Enregistre un backup dans le schéma du Pi
pub async fn save_backup(
    pi_name: &str,
    backup_type: &str,
    service_name: Option<&str>,
    file_path: &str,
    file_size: i64,
    checksum: &str,
    storage_path: &str,
    metadata: Option<serde_json::Value>,
) -> Result<String> {
    let schema_name = pi_name_to_schema(pi_name);
    let client = reqwest::Client::new();
    let supabase_url = get_supabase_url();
    let service_key = get_supabase_service_key();

    let mut body = json!({
        "backup_type": backup_type,
        "file_path": file_path,
        "file_size": file_size,
        "checksum": checksum,
        "storage_path": storage_path
    });

    if let Some(svc) = service_name {
        body["service_name"] = json!(svc);
    }
    if let Some(meta) = metadata {
        body["metadata"] = meta;
    }

    let response = client
        .post(format!("{}/rest/v1/backups", supabase_url))
        .header("apikey", &service_key)
        .header("Authorization", format!("Bearer {}", service_key))
        .header("Content-Type", "application/json")
        .header("Content-Profile", &schema_name)
        .header("Prefer", "return=representation")
        .json(&body)
        .send()
        .await?;

    #[derive(Deserialize)]
    struct BackupRow {
        id: String,
    }

    let result: Vec<BackupRow> = response.json().await?;
    let id = result.first().map(|b| b.id.clone()).unwrap_or_default();

    println!("[Supabase] Saved backup in schema '{}': {}", schema_name, id);
    Ok(id)
}

// =============================================================================
// CATALOGUE MEDIA
// =============================================================================

/// Type de média
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaType {
    Movie,
    Series,
    Episode,
}

/// Ajoute ou met à jour un film/série dans le catalogue
pub async fn upsert_media(
    pi_name: &str,
    media_type: MediaType,
    title: &str,
    year: Option<i32>,
    imdb_id: Option<&str>,
    tmdb_id: Option<i32>,
    file_path: Option<&str>,
    file_size: Option<i64>,
    quality: Option<&str>,
    debrid_link: Option<&str>,
    poster_url: Option<&str>,
    overview: Option<&str>,
    metadata: Option<serde_json::Value>,
) -> Result<String> {
    let schema_name = pi_name_to_schema(pi_name);
    let client = reqwest::Client::new();
    let supabase_url = get_supabase_url();
    let service_key = get_supabase_service_key();

    let media_type_str = match media_type {
        MediaType::Movie => "movie",
        MediaType::Series => "series",
        MediaType::Episode => "episode",
    };

    let mut body = json!({
        "media_type": media_type_str,
        "title": title
    });

    if let Some(y) = year { body["year"] = json!(y); }
    if let Some(id) = imdb_id { body["imdb_id"] = json!(id); }
    if let Some(id) = tmdb_id { body["tmdb_id"] = json!(id); }
    if let Some(path) = file_path { body["file_path"] = json!(path); }
    if let Some(size) = file_size { body["file_size"] = json!(size); }
    if let Some(q) = quality { body["quality"] = json!(q); }
    if let Some(link) = debrid_link { body["debrid_link"] = json!(link); }
    if let Some(poster) = poster_url { body["poster_url"] = json!(poster); }
    if let Some(desc) = overview { body["overview"] = json!(desc); }
    if let Some(meta) = metadata { body["metadata"] = meta; }

    // Upsert basé sur imdb_id ou tmdb_id si présent
    let response = client
        .post(format!("{}/rest/v1/media", supabase_url))
        .header("apikey", &service_key)
        .header("Authorization", format!("Bearer {}", service_key))
        .header("Content-Type", "application/json")
        .header("Content-Profile", &schema_name)
        .header("Prefer", "return=representation")
        .json(&body)
        .send()
        .await?;

    #[derive(Deserialize)]
    struct MediaRow { id: String }

    let result: Vec<MediaRow> = response.json().await?;
    let id = result.first().map(|m| m.id.clone()).unwrap_or_default();

    println!("[Supabase] Saved media '{}' in schema '{}': {}", title, schema_name, id);
    Ok(id)
}

/// Ajoute un épisode à une série existante
pub async fn add_episode(
    pi_name: &str,
    series_id: &str,
    season_number: i32,
    episode_number: i32,
    episode_title: &str,
    file_path: Option<&str>,
    file_size: Option<i64>,
    debrid_link: Option<&str>,
) -> Result<String> {
    let schema_name = pi_name_to_schema(pi_name);
    let client = reqwest::Client::new();
    let supabase_url = get_supabase_url();
    let service_key = get_supabase_service_key();

    let mut body = json!({
        "media_type": "episode",
        "title": episode_title,
        "episode_title": episode_title,
        "series_id": series_id,
        "season_number": season_number,
        "episode_number": episode_number
    });

    if let Some(path) = file_path { body["file_path"] = json!(path); }
    if let Some(size) = file_size { body["file_size"] = json!(size); }
    if let Some(link) = debrid_link { body["debrid_link"] = json!(link); }

    let response = client
        .post(format!("{}/rest/v1/media", supabase_url))
        .header("apikey", &service_key)
        .header("Authorization", format!("Bearer {}", service_key))
        .header("Content-Type", "application/json")
        .header("Content-Profile", &schema_name)
        .header("Prefer", "return=representation")
        .json(&body)
        .send()
        .await?;

    #[derive(Deserialize)]
    struct MediaRow { id: String }

    let result: Vec<MediaRow> = response.json().await?;
    Ok(result.first().map(|m| m.id.clone()).unwrap_or_default())
}

/// Met à jour le lien debrid d'un média
pub async fn update_media_debrid_link(
    pi_name: &str,
    media_id: &str,
    debrid_link: &str,
    expires_at: Option<&str>,
) -> Result<()> {
    let schema_name = pi_name_to_schema(pi_name);
    let client = reqwest::Client::new();
    let supabase_url = get_supabase_url();
    let service_key = get_supabase_service_key();

    let mut body = json!({
        "debrid_link": debrid_link
    });

    if let Some(exp) = expires_at {
        body["debrid_link_expires"] = json!(exp);
    }

    client
        .patch(format!(
            "{}/rest/v1/media?id=eq.{}",
            supabase_url, media_id
        ))
        .header("apikey", &service_key)
        .header("Authorization", format!("Bearer {}", service_key))
        .header("Content-Type", "application/json")
        .header("Content-Profile", &schema_name)
        .json(&body)
        .send()
        .await?;

    Ok(())
}

/// Marque un média comme regardé
pub async fn mark_media_watched(
    pi_name: &str,
    media_id: &str,
    progress_seconds: Option<i32>,
) -> Result<()> {
    let schema_name = pi_name_to_schema(pi_name);
    let client = reqwest::Client::new();
    let supabase_url = get_supabase_url();
    let service_key = get_supabase_service_key();

    let mut body = json!({
        "watched": true,
        "watched_at": chrono::Utc::now().to_rfc3339(),
        "status": "watched"
    });

    if let Some(progress) = progress_seconds {
        body["watch_progress"] = json!(progress);
    }

    client
        .patch(format!(
            "{}/rest/v1/media?id=eq.{}",
            supabase_url, media_id
        ))
        .header("apikey", &service_key)
        .header("Authorization", format!("Bearer {}", service_key))
        .header("Content-Type", "application/json")
        .header("Content-Profile", &schema_name)
        .json(&body)
        .send()
        .await?;

    Ok(())
}

// =============================================================================
// TÉLÉCHARGEMENTS
// =============================================================================

/// Crée un téléchargement
pub async fn create_download(
    pi_name: &str,
    media_id: &str,
    source: &str,
    source_url: Option<&str>,
    torrent_hash: Option<&str>,
    total_size: Option<i64>,
) -> Result<String> {
    let schema_name = pi_name_to_schema(pi_name);
    let client = reqwest::Client::new();
    let supabase_url = get_supabase_url();
    let service_key = get_supabase_service_key();

    let mut body = json!({
        "media_id": media_id,
        "source": source,
        "status": "pending"
    });

    if let Some(url) = source_url { body["source_url"] = json!(url); }
    if let Some(hash) = torrent_hash { body["torrent_hash"] = json!(hash); }
    if let Some(size) = total_size { body["total_size"] = json!(size); }

    let response = client
        .post(format!("{}/rest/v1/downloads", supabase_url))
        .header("apikey", &service_key)
        .header("Authorization", format!("Bearer {}", service_key))
        .header("Content-Type", "application/json")
        .header("Content-Profile", &schema_name)
        .header("Prefer", "return=representation")
        .json(&body)
        .send()
        .await?;

    #[derive(Deserialize)]
    struct DownloadRow { id: String }

    let result: Vec<DownloadRow> = response.json().await?;
    Ok(result.first().map(|d| d.id.clone()).unwrap_or_default())
}

/// Met à jour la progression d'un téléchargement
pub async fn update_download_progress(
    pi_name: &str,
    download_id: &str,
    status: &str,
    progress: f64,
    download_speed: Option<i64>,
    downloaded_size: Option<i64>,
    seeds: Option<i32>,
    peers: Option<i32>,
) -> Result<()> {
    let schema_name = pi_name_to_schema(pi_name);
    let client = reqwest::Client::new();
    let supabase_url = get_supabase_url();
    let service_key = get_supabase_service_key();

    let mut body = json!({
        "status": status,
        "progress": progress
    });

    if let Some(speed) = download_speed { body["download_speed"] = json!(speed); }
    if let Some(size) = downloaded_size { body["downloaded_size"] = json!(size); }
    if let Some(s) = seeds { body["seeds"] = json!(s); }
    if let Some(p) = peers { body["peers"] = json!(p); }

    // Ajouter started_at si on passe en downloading
    if status == "downloading" {
        body["started_at"] = json!(chrono::Utc::now().to_rfc3339());
    }

    // Ajouter completed_at si terminé
    if status == "completed" {
        body["completed_at"] = json!(chrono::Utc::now().to_rfc3339());
    }

    client
        .patch(format!(
            "{}/rest/v1/downloads?id=eq.{}",
            supabase_url, download_id
        ))
        .header("apikey", &service_key)
        .header("Authorization", format!("Bearer {}", service_key))
        .header("Content-Type", "application/json")
        .header("Content-Profile", &schema_name)
        .json(&body)
        .send()
        .await?;

    Ok(())
}
