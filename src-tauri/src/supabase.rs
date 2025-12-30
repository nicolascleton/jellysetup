use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::json;

// Ces valeurs sont injectées au build via .env
// Pour build: créer .env avec SUPABASE_URL et SUPABASE_ANON_KEY
fn get_supabase_url() -> String {
    option_env!("SUPABASE_URL")
        .unwrap_or("https://your-project.supabase.co")
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

#[derive(Debug, Serialize, Deserialize)]
struct Installation {
    id: Option<String>,
    pi_name: String,
    local_ip: String,
    ssh_public_key: String,
    ssh_private_key_encrypted: String,
    status: String,
    installer_version: String,
}

/// Sauvegarde une installation dans Supabase
pub async fn save_installation(
    pi_name: &str,
    pi_ip: &str,
    ssh_public_key: &str,
    ssh_private_key_encrypted: &str,
    installer_version: &str,
) -> Result<String> {
    let client = reqwest::Client::new();

    let body = json!({
        "pi_name": pi_name,
        "local_ip": pi_ip,
        "ssh_public_key": ssh_public_key,
        "ssh_private_key_encrypted": ssh_private_key_encrypted,
        "status": "pending",
        "installer_version": installer_version
    });

    let supabase_url = get_supabase_url();
    let supabase_key = get_supabase_key();

    let response = client
        .post(format!("{}/rest/v1/installations", supabase_url))
        .header("apikey", &supabase_key)
        .header("Authorization", format!("Bearer {}", supabase_key))
        .header("Content-Type", "application/json")
        .header("Prefer", "return=representation")
        .json(&body)
        .send()
        .await?;

    let result: Vec<Installation> = response.json().await?;

    Ok(result.first().and_then(|i| i.id.clone()).unwrap_or_default())
}

/// Met à jour le statut d'une installation
pub async fn update_status(installation_id: &str, status: &str, error: Option<&str>) -> Result<()> {
    let client = reqwest::Client::new();
    let supabase_url = get_supabase_url();
    let supabase_key = get_supabase_key();

    let mut body = json!({
        "status": status,
        "last_seen": chrono::Utc::now().to_rfc3339()
    });

    if let Some(err) = error {
        body["error_message"] = json!(err);
    }

    client
        .patch(format!(
            "{}/rest/v1/installations?id=eq.{}",
            supabase_url, installation_id
        ))
        .header("apikey", &supabase_key)
        .header("Authorization", format!("Bearer {}", supabase_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await?;

    Ok(())
}

/// Ajoute un log d'installation
pub async fn add_log(
    installation_id: &str,
    step: &str,
    status: &str,
    message: &str,
    duration_ms: Option<i64>,
) -> Result<()> {
    let client = reqwest::Client::new();
    let supabase_url = get_supabase_url();
    let supabase_key = get_supabase_key();

    let body = json!({
        "installation_id": installation_id,
        "step": step,
        "status": status,
        "message": message,
        "duration_ms": duration_ms
    });

    client
        .post(format!("{}/rest/v1/installation_logs", supabase_url))
        .header("apikey", &supabase_key)
        .header("Authorization", format!("Bearer {}", supabase_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await?;

    Ok(())
}

/// Vérifie si une installation existe déjà pour ce Pi
pub async fn check_existing(pi_name: &str) -> Result<Option<String>> {
    let client = reqwest::Client::new();
    let supabase_url = get_supabase_url();
    let supabase_key = get_supabase_key();

    let response = client
        .get(format!(
            "{}/rest/v1/installations?pi_name=eq.{}&select=id,status",
            supabase_url, pi_name
        ))
        .header("apikey", &supabase_key)
        .header("Authorization", format!("Bearer {}", supabase_key))
        .send()
        .await?;

    let results: Vec<Installation> = response.json().await?;

    Ok(results.first().and_then(|i| i.id.clone()))
}
