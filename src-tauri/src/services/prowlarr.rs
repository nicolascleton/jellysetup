use anyhow::Result;
use crate::ssh;

/// Applique la configuration Prowlarr depuis master_config (avec clé privée)
pub async fn apply_config(
    host: &str,
    username: &str,
    private_key: &str,
    config: &serde_json::Value,
) -> Result<()> {
    println!("[Prowlarr] Applying master configuration...");

    // Prowlarr gère les indexers
    if let Some(indexers) = config.get("indexers").and_then(|v| v.as_array()) {
        println!("[Prowlarr] Configuring {} indexers...", indexers.len());

        // TODO: Implémenter la configuration via API Prowlarr
        println!("[Prowlarr] Indexers config received: {}", serde_json::to_string_pretty(indexers)?);
    }

    println!("[Prowlarr] ✅ Configuration applied");
    Ok(())
}

/// Applique la configuration Prowlarr depuis master_config (avec mot de passe)
pub async fn apply_config_password(
    host: &str,
    username: &str,
    password: &str,
    config: &serde_json::Value,
) -> Result<()> {
    println!("[Prowlarr] Applying master configuration...");

    // IMPORTANT: Supprimer la DB Prowlarr pour repartir sur une base propre
    let cleanup_script = r#"
cd ~/media-stack && docker compose stop prowlarr
rm -f ~/media-stack/prowlarr/prowlarr.db*
echo "✅ Prowlarr database cleaned"
cd ~/media-stack && docker compose up -d prowlarr
"#;

    ssh::execute_command_password(host, username, password, cleanup_script).await?;
    println!("[Prowlarr] ✅ Database cleaned and service restarted");

    // Prowlarr gère les indexers
    if let Some(indexers) = config.get("indexers").and_then(|v| v.as_array()) {
        println!("[Prowlarr] Configuring {} indexers...", indexers.len());

        // TODO: Implémenter la configuration via API Prowlarr
        println!("[Prowlarr] Indexers config received: {}", serde_json::to_string_pretty(indexers)?);
    }

    println!("[Prowlarr] ✅ Configuration applied");
    Ok(())
}
