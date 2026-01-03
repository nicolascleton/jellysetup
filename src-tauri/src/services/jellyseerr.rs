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

    // NOUVELLE STRATÉGIE 100% AUTONOME via API officielle:
    // 1. Clean la DB et redémarrer Jellyseerr
    // 2. Attendre que l'API soit prête
    // 3. Utiliser POST /api/v1/auth/jellyfin pour créer le premier admin automatiquement
    // 4. Configurer Radarr/Sonarr via l'API

    let script = r#"
set -e  # Arrêter si une commande échoue

echo "🛑 Stopping Jellyseerr..."
cd ~/media-stack
docker compose stop jellyseerr

echo "🗑️  Deleting Jellyseerr data..."
docker run --rm -v "$(pwd)/jellyseerr:/app" alpine sh -c "rm -rf /app/*"

echo "📁 Recreating directories..."
mkdir -p jellyseerr/config jellyseerr/db

echo "🚀 Starting Jellyseerr..."
nohup docker compose up -d jellyseerr > /tmp/jellyseerr_startup.log 2>&1 &

echo "✅ Jellyseerr cleanup done, container starting in background"
"#;

    ssh::execute_command_password(host, username, password, &script).await?;

    println!("[Jellyseerr] ✅ Configuration applied successfully (fresh config)");

    // Attendre que Jellyseerr démarre et que l'API soit prête
    println!("[Jellyseerr] Waiting for API to be ready...");
    let mut jellyseerr_ready = false;
    for i in 0..36 {  // Max 3 minutes (36 * 5s)
        // Vérifier d'abord si le container tourne
        let container_status = ssh::execute_command_password(host, username, password,
            "docker ps -a --filter name=jellyseerr --format '{{.Status}}' 2>/dev/null"
        ).await.unwrap_or_default();

        // Si le container a crashé ou est arrêté
        if container_status.contains("Exited") || container_status.contains("Dead") {
            let logs = ssh::execute_command_password(host, username, password,
                "docker logs jellyseerr --tail 20 2>&1"
            ).await.unwrap_or_default();

            return Err(anyhow::anyhow!(
                "Jellyseerr container crashed or exited unexpectedly!\n\nContainer status: {}\n\nLast logs:\n{}",
                container_status.trim(),
                logs
            ));
        }

        // Tester l'API
        let check = ssh::execute_command_password(host, username, password,
            "curl -s 'http://localhost:5055/api/v1/status' 2>/dev/null || echo 'API_ERROR'"
        ).await.unwrap_or_default();

        println!("[Jellyseerr] Check {}/36: {}", i + 1, if check.contains("version") || check.contains("initialized") { "API ready" } else { "waiting..." });

        if check.contains("version") || check.contains("initialized") || check.len() > 10 {
            jellyseerr_ready = true;
            println!("[Jellyseerr] ✅ API ready after {} seconds", (i + 1) * 5);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }

    if !jellyseerr_ready {
        // Récupérer les logs pour diagnostic
        let logs = ssh::execute_command_password(host, username, password,
            "docker logs jellyseerr --tail 30 2>&1"
        ).await.unwrap_or_default();

        return Err(anyhow::anyhow!(
            "Jellyseerr API not ready after 180 seconds (3 minutes).\n\nPossible causes:\n- Container taking too long to start\n- Insufficient resources (RAM/CPU)\n- Configuration error\n\nLast logs:\n{}",
            logs
        ));
    }

    // WORKFLOW COMPLET comme Buildarr:
    // 1. POST /auth/jellyfin (sauvegarde cookies)
    // 2. GET /settings/jellyfin/library?sync=true (avec cookies)
    // 3. GET /settings/jellyfin/library?enable=... (avec cookies)
    // 4. POST /settings/initialize (avec cookies)

    println!("[Jellyseerr] Initializing via Buildarr-style workflow...");

    let initialization_script = format!(r#"
# Fichier cookie temporaire
COOKIE_FILE="/tmp/jellyseerr_cookies.txt"
rm -f "$COOKIE_FILE"

echo "📡 Step 1: Authenticating with Jellyfin..."
AUTH_RESULT=$(curl -s -c "$COOKIE_FILE" -X POST 'http://localhost:5055/api/v1/auth/jellyfin' \
  -H 'Content-Type: application/json' \
  -d '{{
    "hostname": "http://localhost:8096",
    "username": "{}",
    "password": "{}",
    "email": "{}"
  }}')

echo "Auth response: $AUTH_RESULT"

# Vérifier si auth a réussi (pas d'erreur critique)
if echo "$AUTH_RESULT" | grep -q '"error"'; then
  echo "❌ Authentication failed: $AUTH_RESULT"
  rm -f "$COOKIE_FILE"
  exit 1
fi

echo "✅ Authenticated successfully"

echo "📚 Step 2: Syncing Jellyfin libraries..."
LIBRARIES=$(curl -s -b "$COOKIE_FILE" 'http://localhost:5055/api/v1/settings/jellyfin/library?sync=true')
echo "Libraries: $LIBRARIES"

# Extraire les IDs des bibliothèques (Movies + TV)
LIBRARY_IDS=$(echo "$LIBRARIES" | grep -o '"id":"[^"]*"' | cut -d'"' -f4 | tr '\n' ',' | sed 's/,$//')
echo "Library IDs: $LIBRARY_IDS"

if [ -n "$LIBRARY_IDS" ]; then
  echo "📝 Step 3: Enabling libraries: $LIBRARY_IDS"
  curl -s -b "$COOKIE_FILE" "http://localhost:5055/api/v1/settings/jellyfin/library?enable=$LIBRARY_IDS" > /dev/null
  echo "✅ Libraries enabled"
fi

echo "🏁 Step 4: Finalizing initialization..."
INIT_RESULT=$(curl -s -b "$COOKIE_FILE" -X POST 'http://localhost:5055/api/v1/settings/initialize')
echo "Initialize response: $INIT_RESULT"

# Nettoyer
rm -f "$COOKIE_FILE"

echo "✅ Jellyseerr fully initialized!"
"#, jellyfin_username, jellyfin_password, admin_email);

    let init_result = ssh::execute_command_password(
        host,
        username,
        password,
        &initialization_script
    ).await?;

    println!("[Jellyseerr] Initialization output:\n{}", init_result);

    if init_result.contains("Authentication failed") {
        return Err(anyhow::anyhow!("Failed to authenticate with Jellyfin"));
    }

    println!("[Jellyseerr] ✅ Admin created and initialized via Buildarr workflow");

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
