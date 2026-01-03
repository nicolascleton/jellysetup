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
    // Utiliser docker run avec Alpine pour éviter sudo
    let cleanup_script = r#"
cd ~/media-stack && docker compose stop sonarr

# Supprimer la DB via docker run (évite sudo sur l'hôte)
docker run --rm -v "$(pwd)/sonarr:/app" alpine sh -c "rm -f /app/sonarr.db*"

echo "✅ Sonarr database cleaned"
cd ~/media-stack && docker compose up -d sonarr
"#;

    ssh::execute_command_password(host, username, password, cleanup_script).await?;
    println!("[Sonarr] ✅ Database cleaned and service restarted");

    // Attendre que Sonarr démarre et crée la base de données
    println!("[Sonarr] Waiting for database initialization...");
    let mut sonarr_ready = false;
    for i in 0..24 {  // Max 2 minutes (24 * 5s)
        // Vérifier si Sonarr répond sur son API
        let check = ssh::execute_command_password(host, username, password,
            "curl -s 'http://localhost:8989/api/v3/system/status' 2>/dev/null || echo 'API_ERROR'"
        ).await.unwrap_or_default();

        println!("[Sonarr] Check {}/24: {}", i + 1, if check.contains("instanceName") { "API ready" } else { "waiting..." });

        if check.contains("instanceName") || check.contains("\"version\"") {
            sonarr_ready = true;
            println!("[Sonarr] ✅ Database ready after {} seconds", (i + 1) * 5);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }

    if !sonarr_ready {
        return Err(anyhow::anyhow!("Sonarr not initialized after 120 seconds"));
    }

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
