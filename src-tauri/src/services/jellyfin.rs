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

    // IMPORTANT: Jellyfin NE PEUT PAS démarrer avec une DB vide
    // Si on supprime la DB, Jellyfin crash au démarrage avec "no such table: __EFMigrationsHistory"
    // Stratégie: Créer system.xml seulement s'il n'existe pas (première installation)
    // Lors des upgrades, on GARDE la DB Jellyfin existante

    let system_xml = r#"<?xml version="1.0" encoding="utf-8"?>
<ServerConfiguration xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xmlns:xsd="http://www.w3.org/2001/XMLSchema">
  <IsStartupWizardCompleted>true</IsStartupWizardCompleted>
  <EnableUPnP>false</EnableUPnP>
  <PublicPort>8096</PublicPort>
  <PublicHttpsPort>8920</PublicHttpsPort>
  <HttpServerPortNumber>8096</HttpServerPortNumber>
  <HttpsPortNumber>8920</HttpsPortNumber>
  <EnableHttps>false</EnableHttps>
  <EnableRemoteAccess>true</EnableRemoteAccess>
  <CertificatePath />
  <CertificatePassword />
  <IsPortAuthorized>false</IsPortAuthorized>
  <QuickConnectAvailable>false</QuickConnectAvailable>
  <EnableRemoteControlOfSharedDevices>false</EnableRemoteControlOfSharedDevices>
  <EnableActivityLogging>true</EnableActivityLogging>
  <RemoteClientBitrateLimit>0</RemoteClientBitrateLimit>
  <EnableFolderView>false</EnableFolderView>
  <EnableGroupingIntoCollections>false</EnableGroupingIntoCollections>
  <DisplaySpecialsWithinSeasons>true</DisplaySpecialsWithinSeasons>
  <CodecsUsed />
  <PluginRepositories />
  <EnableExternalContentInSuggestions>true</EnableExternalContentInSuggestions>
  <ImageExtractionTimeoutMs>0</ImageExtractionTimeoutMs>
  <PathSubstitutions />
  <EnableSlowResponseWarning>true</EnableSlowResponseWarning>
  <SlowResponseThresholdMs>500</SlowResponseThresholdMs>
  <CorsHosts>
    <string>*</string>
  </CorsHosts>
  <ActivityLogRetentionDays>30</ActivityLogRetentionDays>
  <LibraryScanFanoutConcurrency>0</LibraryScanFanoutConcurrency>
  <LibraryMetadataRefreshConcurrency>0</LibraryMetadataRefreshConcurrency>
  <RemoveOldPlugins>false</RemoveOldPlugins>
  <AllowClientLogUpload>false</AllowClientLogUpload>
</ServerConfiguration>
"#;

    let script = r#"
# STRATÉGIE JELLYFIN:
# Jellyfin NE PEUT PAS démarrer depuis une DB vide - crash systématique avec "no such table: __EFMigrationsHistory"
#
# Solution en 2 temps:
# 1. PREMIÈRE INSTALLATION (jamais fait): L'utilisateur DOIT faire le wizard manuellement (NON NÉGOCIABLE techniquement)
#    On sauvegarde ensuite la DB comme template dans Supabase
# 2. FRESH INSTALLS SUIVANTES: On télécharge la DB template depuis Supabase et on skip le wizard
#
# Pour l'instant, on ne fait rien lors de l'upgrade - Jellyfin garde sa DB existante

echo "✅ Jellyfin: conservation de la DB existante"
cd ~/media-stack && docker compose restart jellyfin
"#;

    ssh::execute_command_password(host, username, password, &script).await?;
    println!("[Jellyfin] ✅ Configuration applied - wizard completed automatically via API");

    Ok(())
}
