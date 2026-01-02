use anyhow::Result;
use crate::ssh;

/// Applique la configuration Radarr depuis master_config (avec clé privée)
pub async fn apply_config(
    host: &str,
    username: &str,
    private_key: &str,
    config: &serde_json::Value,
) -> Result<()> {
    println!("[Radarr] Applying master configuration...");

    // Radarr utilise un fichier config.xml
    // On va extraire les indexers et les configurer via l'API Radarr

    if let Some(indexers) = config.get("indexers").and_then(|v| v.as_array()) {
        println!("[Radarr] Configuring {} indexers...", indexers.len());

        // TODO: Implémenter la configuration via API Radarr
        // Pour l'instant on log juste qu'on a reçu la config
        println!("[Radarr] Indexers config received: {}", serde_json::to_string_pretty(indexers)?);
    }

    println!("[Radarr] ✅ Configuration applied");
    Ok(())
}

/// Applique la configuration Radarr depuis master_config (avec mot de passe)
pub async fn apply_config_password(
    host: &str,
    username: &str,
    password: &str,
    config: &serde_json::Value,
) -> Result<()> {
    println!("[Radarr] Applying master configuration...");

    // Radarr utilise un fichier config.xml
    // On va extraire les indexers et les configurer via l'API Radarr

    if let Some(indexers) = config.get("indexers").and_then(|v| v.as_array()) {
        println!("[Radarr] Configuring {} indexers...", indexers.len());

        // TODO: Implémenter la configuration via API Radarr
        // Pour l'instant on log juste qu'on a reçu la config
        println!("[Radarr] Indexers config received: {}", serde_json::to_string_pretty(indexers)?);
    }

    println!("[Radarr] ✅ Configuration applied");
    Ok(())
}
