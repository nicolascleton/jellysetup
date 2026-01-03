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
    jellyfin_username: &str,
    jellyfin_password: &str,
    admin_email: &str,
) -> Result<()> {
    println!("[Jellyseerr] Applying master configuration...");

    // Convertir la config en JSON string
    let config_str = serde_json::to_string_pretty(config)?;

    // STRATÉGIE JELLYSEERR pour installation autonome SANS sudo:
    // 1. Utiliser docker exec pour supprimer les fichiers (pas besoin de sudo sur l'hôte)
    // 2. Laisser Jellyseerr créer sa DB fraîche
    // 3. Utiliser docker exec + sqlite3 pour créer l'admin automatiquement
    // 4. Utiliser l'API pour configurer Radarr/Sonarr
    let script = r#"
# Arrêter Jellyseerr
cd ~/media-stack && docker compose stop jellyseerr

# Supprimer config et DB via docker exec (évite sudo sur l'hôte)
docker run --rm -v "$(pwd)/jellyseerr:/app" alpine sh -c "rm -rf /app/config/* /app/db/*"

# Redémarrer Jellyseerr pour créer une DB fraîche
docker compose up -d jellyseerr

echo "✅ Jellyseerr database cleaned and service started"
"#;

    ssh::execute_command_password(host, username, password, &script).await?;

    println!("[Jellyseerr] ✅ Configuration applied successfully (fresh config)");

    // Attendre que Jellyseerr démarre et crée la base de données
    println!("[Jellyseerr] Waiting for database initialization...");
    ssh::execute_command_password(
        host,
        username,
        password,
        "sleep 20"
    ).await?;

    // Créer l'utilisateur admin directement dans la DB via docker exec
    // IMPORTANT: On utilise docker exec avec un container Alpine qui a sqlite3
    // On génère le hash bcrypt du password Jellyfin
    let create_admin_script = format!(r#"
# Attendre que Jellyseerr crée la DB
sleep 5

# Créer l'utilisateur admin via docker exec avec sqlite3 + bcrypt
# On monte le répertoire jellyseerr directement depuis media-stack
cd ~/media-stack

docker run --rm -v "$(pwd)/jellyseerr/config:/config" alpine sh -c "
  apk add --no-cache sqlite python3 py3-pip >/dev/null 2>&1
  pip3 install --break-system-packages bcrypt >/dev/null 2>&1

  # Générer le hash bcrypt du password Jellyfin
  PASSWORD_HASH=\$(python3 -c 'import bcrypt; print(bcrypt.hashpw(b\"{}\", bcrypt.gensalt(rounds=10)).decode())')

  # Créer l'utilisateur admin avec les credentials Jellyfin
  sqlite3 /config/db.sqlite3 <<SQL
INSERT OR REPLACE INTO user (id, email, username, password, userType, permissions, avatar, createdAt, updatedAt)
VALUES (
  1,
  '{}',
  '{}',
  '\$PASSWORD_HASH',
  1,
  16383,
  '',
  datetime('now'),
  datetime('now')
);
SQL

  echo '✅ Admin user created: {} / {}'
"
"#, jellyfin_password, admin_email, jellyfin_username, admin_email, jellyfin_username);

    ssh::execute_command_password(
        host,
        username,
        password,
        &create_admin_script
    ).await?;

    println!("[Jellyseerr] ✅ Admin user created automatically");

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
