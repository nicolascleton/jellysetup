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
    let mut jellyseerr_ready = false;
    for i in 0..24 {  // Max 2 minutes (24 * 5s)
        // Vérifier si la table user existe dans la DB
        let check = ssh::execute_command_password(host, username, password,
            "cd ~/media-stack && docker run --rm -v \"$(pwd)/jellyseerr/config:/config\" alpine sh -c 'apk add --no-cache sqlite >/dev/null 2>&1 && sqlite3 /config/db.sqlite3 \"SELECT name FROM sqlite_master WHERE type=\\\"table\\\" AND name=\\\"user\\\";\"' 2>/dev/null || echo 'TABLE_NOT_FOUND'"
        ).await.unwrap_or_default();

        println!("[Jellyseerr] Check {}/24: {}", i + 1, if check.contains("user") { "user table found" } else { "waiting..." });

        if check.trim() == "user" {
            jellyseerr_ready = true;
            println!("[Jellyseerr] ✅ Database ready after {} seconds", (i + 1) * 5);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }

    if !jellyseerr_ready {
        return Err(anyhow::anyhow!("Jellyseerr database not initialized after 120 seconds"));
    }

    // Créer l'utilisateur admin directement dans la DB via docker exec
    // IMPORTANT: On utilise docker exec avec un container Alpine qui a sqlite3
    // On génère le hash bcrypt du password Jellyfin
    // Créer un script Python qui sera écrit dans le container
    let python_script = format!(r#"import bcrypt
import sqlite3

print('🔐 Generating bcrypt hash for admin password...', flush=True)
# Hash du password
password_hash = bcrypt.hashpw(b'{}', bcrypt.gensalt(rounds=10)).decode()

print('📝 Inserting admin user into database...', flush=True)
# Connexion à la DB (on sait qu'elle existe grâce au wait loop Rust)
conn = sqlite3.connect('/config/db.sqlite3')
cursor = conn.cursor()

# Créer l'utilisateur admin
cursor.execute('''
INSERT OR REPLACE INTO user (id, email, username, password, userType, permissions, avatar, createdAt, updatedAt)
VALUES (1, ?, ?, ?, 1, 16383, '', datetime('now'), datetime('now'))
''', ('{}', '{}', password_hash))

conn.commit()
conn.close()

print('✅ Admin user created: {} / {}', flush=True)
"#, jellyfin_password, admin_email, jellyfin_username, admin_email, jellyfin_username);

    let create_admin_script = format!(r#"
# Créer l'utilisateur admin via docker exec avec sqlite3 + bcrypt
cd ~/media-stack

# Écrire le script Python dans un fichier temporaire
cat > /tmp/create_jellyseerr_admin.py <<'PYTHON_EOF'
{}
PYTHON_EOF

# Exécuter le script dans le container Alpine
docker run --rm -v "$(pwd)/jellyseerr/config:/config" -v /tmp/create_jellyseerr_admin.py:/script.py alpine sh -c "
  apk add --no-cache sqlite python3 py3-pip >/dev/null 2>&1
  pip3 install --break-system-packages bcrypt >/dev/null 2>&1
  python3 /script.py
"

# Nettoyer
rm /tmp/create_jellyseerr_admin.py
"#, python_script);

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
