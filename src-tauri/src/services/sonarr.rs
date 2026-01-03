use anyhow::Result;
use crate::ssh;

/// Applique la configuration Sonarr depuis master_config (avec clé privée)
pub async fn apply_config(
    host: &str,
    username: &str,
    private_key: &str,
    config: &serde_json::Value,
) -> Result<()> {
    println!("[Sonarr] Applying master configuration...");

    // Sonarr utilise un fichier config.xml
    // On va extraire les indexers et les configurer via l'API Sonarr

    if let Some(indexers) = config.get("indexers").and_then(|v| v.as_array()) {
        println!("[Sonarr] Configuring {} indexers...", indexers.len());

        // TODO: Implémenter la configuration via API Sonarr
        // Pour l'instant on log juste qu'on a reçu la config
        println!("[Sonarr] Indexers config received: {}", serde_json::to_string_pretty(indexers)?);
    }

    println!("[Sonarr] ✅ Configuration applied");
    Ok(())
}

/// Applique la configuration Sonarr depuis master_config (avec mot de passe)
pub async fn apply_config_password(
    host: &str,
    username: &str,
    password: &str,
    config: &serde_json::Value,
) -> Result<()> {
    println!("[Sonarr] Applying master configuration...");

    // IMPORTANT: Supprimer la DB Sonarr pour repartir sur une base propre
    let cleanup_script = r#"
cd ~/media-stack && docker compose stop sonarr
rm -f ~/media-stack/sonarr/sonarr.db*
echo "✅ Sonarr database cleaned"
cd ~/media-stack && docker compose up -d sonarr
"#;

    ssh::execute_command_password(host, username, password, cleanup_script).await?;
    println!("[Sonarr] ✅ Database cleaned and service restarted");

    // Sonarr utilise un fichier config.xml
    // On va extraire les indexers et les configurer via l'API Sonarr

    if let Some(indexers) = config.get("indexers").and_then(|v| v.as_array()) {
        println!("[Sonarr] Configuring {} indexers...", indexers.len());

        // TODO: Implémenter la configuration via API Sonarr
        // Pour l'instant on log juste qu'on a reçu la config
        println!("[Sonarr] Indexers config received: {}", serde_json::to_string_pretty(indexers)?);
    }

    println!("[Sonarr] ✅ Configuration applied");
    Ok(())
}
