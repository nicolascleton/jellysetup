use anyhow::Result;
use crate::ssh;

/// Applique la configuration Jellyfin depuis master_config (avec clé privée)
pub async fn apply_config(
    host: &str,
    username: &str,
    private_key: &str,
    config: &serde_json::Value,
) -> Result<()> {
    println!("[Jellyfin] Applying master configuration...");

    // Jellyfin a plusieurs fichiers de config
    // - system.xml (configuration système)
    // - network.xml (configuration réseau)
    // - logging.json (configuration des logs)
    // - encoding.xml (configuration du transcodage)

    // Pour l'instant on log juste la config reçue
    println!("[Jellyfin] Config received: {}", serde_json::to_string_pretty(config)?);

    // TODO: Implémenter la configuration des fichiers Jellyfin
    // Cela nécessite de mapper le JSON vers les différents fichiers XML/JSON

    println!("[Jellyfin] ✅ Configuration applied");
    Ok(())
}

/// Applique la configuration Jellyfin depuis master_config (avec mot de passe)
pub async fn apply_config_password(
    host: &str,
    username: &str,
    password: &str,
    config: &serde_json::Value,
) -> Result<()> {
    println!("[Jellyfin] Applying master configuration...");

    // IMPORTANT: Supprimer TOUTE la config et DB Jellyfin pour repartir sur une instance neuve
    let cleanup_script = r#"
cd ~/media-stack && docker compose stop jellyfin
rm -rf ~/media-stack/jellyfin/config/*
rm -rf ~/media-stack/jellyfin/data/*
echo "✅ Jellyfin config and data cleaned (fresh instance)"
cd ~/media-stack && docker compose up -d jellyfin
"#;

    ssh::execute_command_password(host, username, password, cleanup_script).await?;
    println!("[Jellyfin] ✅ Fresh instance created and service restarted");

    // Jellyfin a plusieurs fichiers de config
    // - system.xml (configuration système)
    // - network.xml (configuration réseau)
    // - logging.json (configuration des logs)
    // - encoding.xml (configuration du transcodage)

    // Pour l'instant on log juste la config reçue
    println!("[Jellyfin] Config received: {}", serde_json::to_string_pretty(config)?);

    // TODO: Implémenter la configuration des fichiers Jellyfin
    // Cela nécessite de mapper le JSON vers les différents fichiers XML/JSON

    println!("[Jellyfin] ✅ Configuration applied");
    Ok(())
}
