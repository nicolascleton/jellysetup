pub mod jellyseerr;
pub mod radarr;
pub mod sonarr;
pub mod prowlarr;
pub mod jellyfin;

use anyhow::Result;
use crate::ssh;
use crate::template_engine::TemplateVars;

/// Applique la configuration d'un service sur le Pi via SSH (clé privée)
pub async fn apply_service_config(
    host: &str,
    username: &str,
    private_key: &str,
    service_name: &str,
    config_json: &serde_json::Value,
    vars: &TemplateVars,
) -> Result<()> {
    println!("[Services] Applying {} configuration...", service_name);

    // Remplacer les variables dans la config
    let resolved_config = vars.replace_in_json(config_json);

    // Appliquer la config selon le service
    match service_name {
        "jellyseerr" => jellyseerr::apply_config(host, username, private_key, &resolved_config).await,
        "radarr" => radarr::apply_config(host, username, private_key, &resolved_config).await,
        "sonarr" => sonarr::apply_config(host, username, private_key, &resolved_config).await,
        "prowlarr" => prowlarr::apply_config(host, username, private_key, &resolved_config).await,
        "jellyfin" => jellyfin::apply_config(host, username, private_key, &resolved_config).await,
        _ => {
            println!("[Services] Unknown service: {}", service_name);
            Ok(())
        }
    }
}

/// Applique la configuration d'un service sur le Pi via SSH (mot de passe)
pub async fn apply_service_config_password(
    host: &str,
    username: &str,
    password: &str,
    service_name: &str,
    config_json: &serde_json::Value,
    vars: &TemplateVars,
    jellyfin_username: &str,
    jellyfin_password: &str,
    admin_email: &str,
) -> Result<()> {
    println!("[Services] Applying {} configuration...", service_name);

    // Remplacer les variables dans la config
    let resolved_config = vars.replace_in_json(config_json);

    // Appliquer la config selon le service
    match service_name {
        "jellyseerr" => {
            // Pour Jellyseerr, on a besoin des clés API Radarr/Sonarr pour la config API
            // Extraire les valeurs depuis le JSON résolu (qui contient déjà les vraies API keys)
            let radarr_api = resolved_config.get("radarr")
                .and_then(|arr| arr.as_array())
                .and_then(|arr| arr.first())
                .and_then(|obj| obj.get("apiKey"))
                .and_then(|key| key.as_str())
                .unwrap_or("");

            let sonarr_api = resolved_config.get("sonarr")
                .and_then(|arr| arr.as_array())
                .and_then(|arr| arr.first())
                .and_then(|obj| obj.get("apiKey"))
                .and_then(|key| key.as_str())
                .unwrap_or("");

            jellyseerr::apply_config_password(
                host, username, password, &resolved_config,
                radarr_api, sonarr_api,
                jellyfin_username, jellyfin_password, admin_email
            ).await
        },
        "radarr" => radarr::apply_config_password(host, username, password, &resolved_config).await,
        "sonarr" => sonarr::apply_config_password(host, username, password, &resolved_config).await,
        "prowlarr" => prowlarr::apply_config_password(host, username, password, &resolved_config).await,
        "jellyfin" => jellyfin::apply_config_password(host, username, password, &resolved_config).await,
        _ => {
            println!("[Services] Unknown service: {}", service_name);
            Ok(())
        }
    }
}
