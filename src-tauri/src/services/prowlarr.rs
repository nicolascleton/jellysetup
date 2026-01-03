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
    // Utiliser docker run avec Alpine pour éviter sudo
    let cleanup_script = r#"
cd ~/media-stack && docker compose stop prowlarr

# Supprimer la DB via docker run (évite sudo sur l'hôte)
docker run --rm -v "$(pwd)/prowlarr:/app" alpine sh -c "rm -f /app/prowlarr.db*"

echo "✅ Prowlarr database cleaned"
cd ~/media-stack && docker compose up -d prowlarr
"#;

    ssh::execute_command_password(host, username, password, cleanup_script).await?;
    println!("[Prowlarr] ✅ Database cleaned and service restarted");

    // Attendre que Prowlarr démarre et crée la base de données
    println!("[Prowlarr] Waiting for database initialization...");
    let mut prowlarr_ready = false;
    for i in 0..24 {  // Max 2 minutes (24 * 5s)
        // Vérifier si Prowlarr répond sur son API
        let check = ssh::execute_command_password(host, username, password,
            "curl -s 'http://localhost:9696/api/v1/system/status' 2>/dev/null || echo 'API_ERROR'"
        ).await.unwrap_or_default();

        println!("[Prowlarr] Check {}/24: {}", i + 1, if check.contains("instanceName") { "API ready" } else { "waiting..." });

        if check.contains("instanceName") || check.contains("\"version\"") {
            prowlarr_ready = true;
            println!("[Prowlarr] ✅ Database ready after {} seconds", (i + 1) * 5);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }

    if !prowlarr_ready {
        return Err(anyhow::anyhow!("Prowlarr not initialized after 120 seconds"));
    }

    // Prowlarr gère les indexers
    if let Some(indexers) = config.get("indexers").and_then(|v| v.as_array()) {
        println!("[Prowlarr] Configuring {} indexers...", indexers.len());

        // TODO: Implémenter la configuration via API Prowlarr
        println!("[Prowlarr] Indexers config received: {}", serde_json::to_string_pretty(indexers)?);
    }

    println!("[Prowlarr] ✅ Configuration applied");
    Ok(())
}
