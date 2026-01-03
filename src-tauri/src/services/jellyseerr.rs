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
    radarr_api_key: &str,
    sonarr_api_key: &str,
) -> Result<()> {
    println!("[Jellyseerr] Applying master configuration...");

    // Convertir la config en JSON string
    let config_str = serde_json::to_string_pretty(config)?;

    // IMPORTANT: Supprimer complètement la config et DB existantes pour forcer une réinitialisation
    // Jellyseerr stocke sa config dans settings.json ET dans db.sqlite3
    // Il faut TOUT supprimer (config + db) pour repartir sur une base propre
    let script = format!(r#"
# Arrêter Jellyseerr d'abord
cd ~/media-stack && docker compose stop jellyseerr

# Supprimer toute la config existante (settings.json dans /config)
# Utiliser sudo car les fichiers appartiennent à root
sudo rm -rf ~/media-stack/jellyseerr/config/*
mkdir -p ~/media-stack/jellyseerr/config

# Supprimer la base de données existante (db.sqlite3 dans /db)
# IMPORTANT: sudo requis car les fichiers DB appartiennent à root
sudo rm -rf ~/media-stack/jellyseerr/db/*
mkdir -p ~/media-stack/jellyseerr/db

# Écrire la nouvelle config
cat > ~/media-stack/jellyseerr/config/settings.json <<'JELLYSEERR_CONFIG_EOF'
{}
JELLYSEERR_CONFIG_EOF
chmod 644 ~/media-stack/jellyseerr/config/settings.json
echo "✅ Jellyseerr config written and DB cleaned (fresh install)"
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

    // Créer l'utilisateur admin et mettre à jour les permissions
    // Jellyseerr nécessite qu'un utilisateur se connecte via Jellyfin pour créer le premier compte
    // On va le faire automatiquement en insérant directement dans la DB
    let permissions_script = r#"
# Installer sqlite3 si pas déjà présent
if ! command -v sqlite3 &> /dev/null; then
    sudo apt-get update -qq && sudo apt-get install -y -qq sqlite3 > /dev/null 2>&1
fi

# Attendre que la DB soit créée
sleep 5

# Vérifier si des utilisateurs existent déjà
USER_COUNT=$(sqlite3 ~/media-stack/jellyseerr/db/db.sqlite3 "SELECT COUNT(*) FROM user;" 2>/dev/null || echo "0")

if [ "$USER_COUNT" = "0" ]; then
    # Créer un utilisateur admin local directement dans la DB
    # Cela évite de devoir passer par le wizard de setup
    sqlite3 ~/media-stack/jellyseerr/db/db.sqlite3 <<'SQL'
INSERT INTO user (email, username, plexUsername, jellyfinUsername, plexId, jellyfinUserId, permissions, avatar, createdAt, updatedAt, userType, plexToken, jellyfinAuthToken, jellyfinDeviceId, jellyfinEmailAddress)
VALUES (
    'admin@jellyseerr.local',
    'Admin',
    NULL,
    NULL,
    NULL,
    NULL,
    16383,
    '/os_avatar_4.png',
    datetime('now'),
    datetime('now'),
    1,
    NULL,
    NULL,
    NULL,
    NULL
);
SQL
    echo "✅ Admin user created with auto-approve permissions"
else
    # Des utilisateurs existent déjà, mettre à jour leurs permissions
    sqlite3 ~/media-stack/jellyseerr/db/db.sqlite3 "UPDATE user SET permissions = 16383;" 2>/dev/null
    echo "✅ User permissions updated to 16383 (auto-approve enabled)"
fi
"#;

    ssh::execute_command_password(
        host,
        username,
        password,
        permissions_script
    ).await?;

    println!("[Jellyseerr] ✅ All user permissions set to auto-approve");

    // Configurer Radarr et Sonarr via l'API Jellyseerr
    // Cela garantit que les serveurs sont bien enregistrés dans la base de données
    let api_config_script = format!(r#"
# Récupérer l'API key de Jellyseerr depuis settings.json
API_KEY=$(cat ~/media-stack/jellyseerr/config/settings.json | grep -o '"apiKey":"[^"]*"' | head -1 | cut -d'"' -f4)

# Attendre que Jellyseerr soit prêt
sleep 5

# Configurer Radarr via l'API
curl -s -X POST "http://localhost:5055/api/v1/settings/radarr" \
  -H "X-Api-Key: $API_KEY" \
  -H "Content-Type: application/json" \
  -d '{{
    "name": "Radarr",
    "hostname": "radarr",
    "port": 7878,
    "apiKey": "{}",
    "useSsl": false,
    "activeProfileId": 4,
    "activeProfileName": "HD-1080p",
    "activeDirectory": "/mnt/decypharr/movies",
    "is4k": false,
    "minimumAvailability": "released",
    "isDefault": true,
    "syncEnabled": true
  }}' > /dev/null 2>&1

# Configurer Sonarr via l'API
curl -s -X POST "http://localhost:5055/api/v1/settings/sonarr" \
  -H "X-Api-Key: $API_KEY" \
  -H "Content-Type: application/json" \
  -d '{{
    "name": "Sonarr",
    "hostname": "sonarr",
    "port": 8989,
    "apiKey": "{}",
    "useSsl": false,
    "activeProfileId": 4,
    "activeProfileName": "HD-1080p",
    "activeDirectory": "/mnt/decypharr/tv",
    "is4k": false,
    "enableSeasonFolders": true,
    "isDefault": true,
    "syncEnabled": true
  }}' > /dev/null 2>&1

echo "✅ Radarr and Sonarr configured via API"
"#, radarr_api_key, sonarr_api_key);

    ssh::execute_command_password(
        host,
        username,
        password,
        &api_config_script
    ).await?;

    println!("[Jellyseerr] ✅ Radarr and Sonarr configured via API");

    Ok(())
}
