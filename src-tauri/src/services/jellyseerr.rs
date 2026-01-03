use anyhow::Result;
use crate::ssh;

/// Applique la configuration Jellyseerr depuis master_config (avec clé privée)
pub async fn apply_config(
    host: &str,
    username: &str,
    private_key: &str,
    config: &serde_json::Value,
) -> Result<()> {
    println!("[Jellyseerr] Applying master configuration...");

    // Convertir la config en JSON string
    let config_str = serde_json::to_string_pretty(config)?;

    // Créer un script temporaire pour écrire le fichier settings.json
    let script = format!(r#"
cat > ~/media-stack/jellyseerr/config/settings.json <<'JELLYSEERR_CONFIG_EOF'
{}
JELLYSEERR_CONFIG_EOF
chmod 644 ~/media-stack/jellyseerr/config/settings.json
echo "✅ Jellyseerr config written"
"#, config_str);

    // Écrire la config via SSH
    ssh::execute_command(host, username, private_key, &script).await?;

    println!("[Jellyseerr] ✅ Configuration applied successfully");

    // Redémarrer le container pour appliquer la config
    ssh::execute_command(
        host,
        username,
        private_key,
        "cd ~/media-stack && docker-compose restart jellyseerr"
    ).await?;

    println!("[Jellyseerr] ✅ Container restarted");

    Ok(())
}

/// Applique la configuration Jellyseerr depuis master_config (avec mot de passe)
pub async fn apply_config_password(
    host: &str,
    username: &str,
    password: &str,
    config: &serde_json::Value,
) -> Result<()> {
    println!("[Jellyseerr] Applying master configuration...");

    // Convertir la config en JSON string
    let config_str = serde_json::to_string_pretty(config)?;

    // IMPORTANT: Supprimer complètement la config existante pour forcer une réinitialisation
    // Jellyseerr stocke sa config dans settings.json ET dans db.sqlite3
    // Il faut tout supprimer pour que la nouvelle config soit chargée
    let script = format!(r#"
# Arrêter Jellyseerr d'abord
cd ~/media-stack && docker compose stop jellyseerr

# Supprimer toute la config existante (settings.json + db.sqlite3)
rm -rf ~/media-stack/jellyseerr/config/*
mkdir -p ~/media-stack/jellyseerr/config

# Écrire la nouvelle config
cat > ~/media-stack/jellyseerr/config/settings.json <<'JELLYSEERR_CONFIG_EOF'
{}
JELLYSEERR_CONFIG_EOF
chmod 644 ~/media-stack/jellyseerr/config/settings.json
echo "✅ Jellyseerr config written (fresh install)"
"#, config_str);

    // Écrire la config via SSH
    ssh::execute_command_password(host, username, password, &script).await?;

    println!("[Jellyseerr] ✅ Configuration applied successfully (fresh config)");

    // Redémarrer le container pour charger la nouvelle config
    ssh::execute_command_password(
        host,
        username,
        password,
        "cd ~/media-stack && docker compose up -d jellyseerr"
    ).await?;

    println!("[Jellyseerr] ✅ Container restarted with fresh config");

    // Attendre que Jellyseerr démarre et crée la base de données
    println!("[Jellyseerr] Waiting for database initialization...");
    ssh::execute_command_password(
        host,
        username,
        password,
        "sleep 15"
    ).await?;

    // Mettre à jour les permissions de TOUS les utilisateurs à 16383 (auto-approve)
    // Cela corrige le problème où les utilisateurs Jellyfin SSO ont des permissions limitées
    let permissions_script = r#"
# Installer sqlite3 si pas déjà présent
if ! command -v sqlite3 &> /dev/null; then
    sudo apt-get update -qq && sudo apt-get install -y -qq sqlite3 > /dev/null 2>&1
fi

# Mettre à jour les permissions de tous les utilisateurs
sqlite3 ~/media-stack/jellyseerr/db/db.sqlite3 "UPDATE user SET permissions = 16383;" 2>/dev/null || echo "Database not ready yet"
echo "✅ User permissions updated to 16383 (auto-approve enabled)"
"#;

    ssh::execute_command_password(
        host,
        username,
        password,
        permissions_script
    ).await?;

    println!("[Jellyseerr] ✅ All user permissions set to auto-approve");

    Ok(())
}
