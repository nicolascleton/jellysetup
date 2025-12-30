// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod sd_card;
mod ssh;
mod network;
mod supabase;
mod flash;
mod crypto;

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
    pub ygg_passkey: Option<String>,
    pub discord_webhook: Option<String>,
    pub cloudflare_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlashProgress {
    pub step: String,
    pub percent: u32,
    pub message: String,
    pub speed: Option<String>,
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
    network::discover_raspberry_pi(&hostname, timeout_secs)
        .await
        .map_err(|e| e.to_string())
}

/// Vérifie la connexion SSH au Pi
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

/// Exécute une série de commandes d'installation
#[tauri::command]
async fn run_installation(
    window: Window,
    host: String,
    username: String,
    private_key: String,
    config: InstallConfig,
    hostname: String,
) -> Result<(), String> {
    flash::run_full_installation(window, &host, &username, &private_key, config, &hostname)
        .await
        .map_err(|e| e.to_string())
}

/// Sauvegarde les credentials dans Supabase
#[tauri::command]
async fn save_to_supabase(
    pi_name: String,
    pi_ip: String,
    ssh_public_key: String,
    ssh_private_key_encrypted: String,
    installer_version: String,
) -> Result<String, String> {
    supabase::save_installation(
        &pi_name,
        &pi_ip,
        &ssh_public_key,
        &ssh_private_key_encrypted,
        &installer_version,
    )
    .await
    .map_err(|e| e.to_string())
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
            ssh_exec,
            run_installation,
            save_to_supabase,
            fetch_procedure,
            check_for_updates,
            check_disk_access,
            open_disk_access_settings,
            restart_app,
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
