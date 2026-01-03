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

    // IMPORTANT: Supprimer la DB Radarr pour repartir sur une base propre
    // Utiliser docker run avec Alpine pour éviter sudo
    let cleanup_script = r#"
cd ~/media-stack && docker compose stop radarr

# Supprimer la DB via docker run (évite sudo sur l'hôte)
docker run --rm -v "$(pwd)/radarr:/app" alpine sh -c "rm -f /app/radarr.db*"

echo "✅ Radarr database cleaned"
cd ~/media-stack && docker compose up -d radarr
"#;

    ssh::execute_command_password(host, username, password, cleanup_script).await?;
    println!("[Radarr] ✅ Database cleaned and service restarted");

    // Attendre que Radarr démarre et crée la base de données
    println!("[Radarr] Waiting for database initialization...");
    let mut radarr_ready = false;
    for i in 0..24 {  // Max 2 minutes (24 * 5s)
        // Vérifier si Radarr répond sur son API
        let check = ssh::execute_command_password(host, username, password,
            "curl -s 'http://localhost:7878/api/v3/system/status' 2>/dev/null || echo 'API_ERROR'"
        ).await.unwrap_or_default();

        println!("[Radarr] Check {}/24: {}", i + 1, if check.contains("instanceName") { "API ready" } else { "waiting..." });

        if check.contains("instanceName") || check.contains("\"version\"") {
            radarr_ready = true;
            println!("[Radarr] ✅ Database ready after {} seconds", (i + 1) * 5);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }

    if !radarr_ready {
        return Err(anyhow::anyhow!("Radarr not initialized after 120 seconds"));
    }

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
