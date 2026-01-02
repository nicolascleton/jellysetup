// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod sd_card;
mod ssh;
mod network;
mod supabase;
mod flash;
mod crypto;
mod logging;
mod master_config;
mod template_engine;
mod services;

use serde::{Deserialize, Serialize};
use tauri::{Manager, Window};

// =============================================================================
// Types
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SDCard {
    pub path: String,
    pub name: String,
    pub size: u64,
    pub removable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlashConfig {
    pub sd_path: String,
    // Système
    pub hostname: String,
    pub system_username: String,
    pub system_password: String,
    // WiFi
    pub wifi_ssid: String,
    pub wifi_password: String,
    pub wifi_country: String,
    // Locale
    pub timezone: String,
    pub keymap: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallConfig {
    pub alldebrid_api_key: String,
    pub jellyfin_username: String,
    pub jellyfin_password: String,
    pub jellyfin_server_name: String,
    pub admin_email: Option<String>,
    pub ygg_passkey: Option<String>,
    pub discord_webhook: Option<String>,
    pub cloudflare_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JellyfinAuth {
    pub server_id: String,
    pub access_token: String,
    pub user_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlashProgress {
    pub step: String,
    pub percent: u32,
    pub message: String,
    pub speed: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jellyfin_auth: Option<JellyfinAuth>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SSHCredentials {
    pub public_key: String,
    pub private_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiInfo {
    pub ip: String,
    pub hostname: String,
    pub mac_address: Option<String>,
}

// =============================================================================
// Commands
// =============================================================================

/// Liste les cartes SD disponibles
#[tauri::command]
async fn list_sd_cards() -> Result<Vec<SDCard>, String> {
    sd_card::list_removable_drives()
        .await
        .map_err(|e| e.to_string())
}

/// Vérifie si l'app a accès aux disques (Full Disk Access sur macOS)
#[tauri::command]
fn check_disk_access() -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    {
        // Tester l'accès à un chemin protégé par TCC
        let home = std::env::var("HOME").unwrap_or_default();
        let test_path = format!("{}/Library/Safari", home);

        if std::path::Path::new(&test_path).exists() {
            match std::fs::read_dir(&test_path) {
                Ok(_) => return Ok(true),
                Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => return Ok(false),
                Err(_) => {} // Continuer avec d'autres tests
            }
        }

        // Si Safari n'existe pas, essayer Mail
        let mail_path = format!("{}/Library/Mail", home);
        if std::path::Path::new(&mail_path).exists() {
            match std::fs::read_dir(&mail_path) {
                Ok(_) => return Ok(true),
                Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => return Ok(false),
                Err(_) => {}
            }
        }

        // Si on ne peut pas vérifier, assumer qu'on n'a pas l'accès
        Ok(false)
    }

    #[cfg(not(target_os = "macos"))]
    {
        Ok(true)
    }
}

/// Ouvre les réglages Full Disk Access (macOS)
#[tauri::command]
fn open_disk_access_settings() {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open")
            .args(["x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles"])
            .spawn();
    }
}

/// Génère une paire de clés SSH
#[tauri::command]
async fn generate_ssh_keys() -> Result<SSHCredentials, String> {
    crypto::generate_ssh_keypair()
        .await
        .map_err(|e| e.to_string())
}

/// Flash la carte SD avec Raspberry Pi OS
#[tauri::command]
async fn flash_sd_card(
    window: Window,
    config: FlashConfig,
    ssh_public_key: String,
) -> Result<(), String> {
    flash::flash_raspberry_pi_os(window, config, ssh_public_key)
        .await
        .map_err(|e| e.to_string())
}

/// Découvre le Raspberry Pi sur le réseau
#[tauri::command]
async fn discover_pi(hostname: String, timeout_secs: u64) -> Result<Option<PiInfo>, String> {
    // Log dans un fichier car stdout/stderr sont avalés sur macOS GUI
    use std::io::Write;
    let _ = std::fs::write("/tmp/jellysetup_discovery.log",
        format!("discover_pi CALLED: hostname={}, timeout={}s\n", hostname, timeout_secs));
    let result = network::discover_raspberry_pi(&hostname, timeout_secs)
        .await
        .map_err(|e| {
            println!("[CMD discover_pi] Error: {}", e);
            e.to_string()
        });
    println!("[CMD discover_pi] Result: {:?}", result);
    result
}

/// Vérifie la connexion SSH au Pi (clé privée)
#[tauri::command]
async fn test_ssh_connection(
    host: String,
    username: String,
    private_key: String,
) -> Result<bool, String> {
    ssh::test_connection(&host, &username, &private_key)
        .await
        .map_err(|e| e.to_string())
}

/// Vérifie la connexion SSH au Pi (mot de passe)
#[tauri::command]
async fn test_ssh_connection_password(
    host: String,
    username: String,
    password: String,
) -> Result<bool, String> {
    ssh::test_connection_password(&host, &username, &password)
        .await
        .map_err(|e| e.to_string())
}

/// Exécute une commande SSH sur le Pi
#[tauri::command]
async fn ssh_exec(
    host: String,
    username: String,
    private_key: String,
    command: String,
) -> Result<String, String> {
    ssh::execute_command(&host, &username, &private_key, &command)
        .await
        .map_err(|e| e.to_string())
}

/// Exécute une série de commandes d'installation (clé SSH)
#[tauri::command]
async fn run_installation(
    window: Window,
    host: String,
    username: String,
    private_key: String,
    config: InstallConfig,
) -> Result<(), String> {
    // Extraire le hostname depuis l'adresse (comme pour la version password)
    let hostname = host.replace(".local", "");
    flash::run_full_installation(window, &host, &username, &private_key, config, &hostname)
        .await
        .map_err(|e| e.to_string())
}

/// Exécute une série de commandes d'installation (mot de passe)
#[tauri::command]
async fn run_installation_password(
    window: Window,
    host: String,
    username: String,
    password: String,
    config: InstallConfig,
) -> Result<(), String> {
    flash::run_full_installation_password(window, &host, &username, &password, config)
        .await
        .map_err(|e| e.to_string())
}

/// Sauvegarde les credentials dans Supabase (ne bloque jamais)
#[tauri::command]
async fn save_to_supabase(
    pi_name: String,
    pi_ip: String,
    ssh_public_key: String,
    ssh_private_key_encrypted: String,
    ssh_host_fingerprint: Option<String>,
    installer_version: String,
) -> Result<String, String> {
    match supabase::save_installation(
        &pi_name,
        &pi_ip,
        Some(&ssh_public_key),
        Some(&ssh_private_key_encrypted),
        ssh_host_fingerprint.as_deref(),
        &installer_version,
    )
    .await {
        Ok(id) => Ok(id),
        Err(e) => {
            println!("[Supabase] Warning: save_installation failed: {}", e);
            // Ne pas bloquer l'installation - retourner un ID local
            Ok("local".to_string())
        }
    }
}

/// Récupère la procédure depuis GitHub
#[tauri::command]
async fn fetch_procedure(version: String) -> Result<String, String> {
    let url = format!(
        "https://raw.githubusercontent.com/nicolascleton/jellysetup/main/procedures/{}/steps.json",
        version
    );

    reqwest::get(&url)
        .await
        .map_err(|e| e.to_string())?
        .text()
        .await
        .map_err(|e| e.to_string())
}

/// Vérifie les mises à jour de l'application
#[tauri::command]
async fn check_for_updates() -> Result<Option<String>, String> {
    let url = "https://jellysetup.com/api/version";

    let response = reqwest::get(url)
        .await
        .map_err(|e| e.to_string())?
        .json::<serde_json::Value>()
        .await
        .map_err(|e| e.to_string())?;

    Ok(response.get("latest").and_then(|v| v.as_str()).map(String::from))
}

/// Redémarre l'application
#[tauri::command]
fn restart_app(app_handle: tauri::AppHandle) {
    // En release, utiliser restart natif
    #[cfg(not(debug_assertions))]
    {
        app_handle.restart();
    }

    // En dev mode, on quitte simplement - le hot reload de Vite s'occupe du reste
    // ou l'utilisateur relance manuellement
    #[cfg(debug_assertions)]
    {
        // Juste quitter, le localStorage garde le flag FDA
        app_handle.exit(0);
    }
}

/// Récupère le dernier fingerprint SSH host capturé
#[tauri::command]
fn get_ssh_host_fingerprint() -> Option<String> {
    ssh::get_last_host_fingerprint()
}

/// Nettoie le known_hosts local pour une IP
#[tauri::command]
fn clear_known_hosts(ip: String) -> Result<(), String> {
    ssh::clear_known_hosts_for_ip(&ip).map_err(|e| e.to_string())
}

// =============================================================================
// Main
// =============================================================================

fn main() {
    tracing_subscriber::fmt::init();

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            list_sd_cards,
            generate_ssh_keys,
            flash_sd_card,
            discover_pi,
            test_ssh_connection,
            test_ssh_connection_password,
            ssh_exec,
            run_installation,
            run_installation_password,
            save_to_supabase,
            fetch_procedure,
            check_for_updates,
            check_disk_access,
            open_disk_access_settings,
            restart_app,
            get_ssh_host_fingerprint,
            clear_known_hosts,
        ])
        .setup(|app| {
            let window = app.get_window("main").unwrap();

            // Centrer la fenêtre
            window.center().unwrap();

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
