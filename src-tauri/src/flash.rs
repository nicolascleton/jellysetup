use crate::{FlashConfig, FlashProgress, InstallConfig};
use anyhow::{anyhow, Result};
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::Path;
use tauri::Window;
use tokio::process::Command;

const RPI_OS_URL: &str = "https://downloads.raspberrypi.com/raspios_lite_arm64/images/raspios_lite_arm64-2024-03-15/2024-03-15-raspios-bookworm-arm64-lite.img.xz";
const RPI_OS_SHA256: &str = "..."; // TODO: Mettre le vrai hash

/// Flash Raspberry Pi OS sur la carte SD
pub async fn flash_raspberry_pi_os(
    window: Window,
    config: FlashConfig,
    ssh_public_key: String,
) -> Result<()> {
    let cache_dir = dirs::cache_dir()
        .ok_or_else(|| anyhow!("Cannot find cache directory"))?
        .join("jellysetup");

    fs::create_dir_all(&cache_dir)?;

    let image_path = cache_dir.join("raspios.img.xz");
    let extracted_path = cache_dir.join("raspios.img");

    // Étape 1: Télécharger l'image si nécessaire
    emit_progress(&window, "download", 0, "Vérification de l'image...", None);

    if !image_path.exists() {
        download_image(&window, RPI_OS_URL, &image_path).await?;
    }

    emit_progress(&window, "extract", 30, "Extraction de l'image...", None);

    // Étape 2: Extraire l'image XZ
    if !extracted_path.exists() {
        extract_xz(&image_path, &extracted_path).await?;
    }

    emit_progress(&window, "unmount", 40, "Démontage de la carte SD...", None);

    // Étape 3: Démonter la carte SD
    crate::sd_card::unmount_disk(&config.sd_path).await?;

    emit_progress(&window, "write", 45, "Écriture de l'image...", None);

    // Étape 4: Écrire l'image sur la carte SD
    write_image_to_sd(&window, &extracted_path, &config.sd_path).await?;

    emit_progress(&window, "configure", 85, "Configuration du système...", None);

    // Étape 5: Configurer le boot (SSH, WiFi, hostname)
    configure_boot_partition(&config, &ssh_public_key).await?;

    emit_progress(&window, "eject", 95, "Éjection de la carte...", None);

    // Étape 6: Éjecter
    crate::sd_card::eject_disk(&config.sd_path).await?;

    emit_progress(&window, "complete", 100, "Carte SD prête !", None);

    Ok(())
}

/// Télécharge l'image Raspberry Pi OS
async fn download_image(window: &Window, url: &str, dest: &Path) -> Result<()> {
    let client = reqwest::Client::new();
    let response = client.get(url).send().await?;

    let total_size = response.content_length().unwrap_or(0);
    let mut downloaded: u64 = 0;

    let mut file = BufWriter::new(File::create(dest)?);
    let mut stream = response.bytes_stream();

    use futures_util::StreamExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk)?;

        downloaded += chunk.len() as u64;
        let percent = if total_size > 0 {
            (downloaded * 30 / total_size) as u32
        } else {
            0
        };

        let speed = format!("{:.1} MB/s", downloaded as f64 / 1_000_000.0);
        emit_progress(
            window,
            "download",
            percent,
            &format!("Téléchargement: {:.0}%", percent),
            Some(&speed),
        );
    }

    file.flush()?;
    Ok(())
}

/// Extrait un fichier .xz
async fn extract_xz(src: &Path, dest: &Path) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        Command::new("xz")
            .args(["-dk", src.to_str().unwrap()])
            .output()
            .await?;
    }

    #[cfg(target_os = "windows")]
    {
        // Utiliser 7z sur Windows
        Command::new("7z")
            .args(["x", "-y", src.to_str().unwrap(), &format!("-o{}", dest.parent().unwrap().display())])
            .output()
            .await?;
    }

    #[cfg(target_os = "linux")]
    {
        Command::new("xz")
            .args(["-dk", src.to_str().unwrap()])
            .output()
            .await?;
    }

    Ok(())
}

/// Écrit l'image sur la carte SD avec progression
async fn write_image_to_sd(window: &Window, image: &Path, sd_path: &str) -> Result<()> {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        // Utiliser dd avec status=progress
        let mut child = Command::new("sudo")
            .args([
                "dd",
                &format!("if={}", image.display()),
                &format!("of={}", sd_path),
                "bs=4M",
                "status=progress",
            ])
            .spawn()?;

        // TODO: Parser la sortie pour la progression
        child.wait().await?;
    }

    #[cfg(target_os = "windows")]
    {
        // Sur Windows, utiliser Win32 API pour écrire directement
        // ou utiliser rufus CLI / wimlib
        // Pour simplifier, on utilise PowerShell avec dd port
        Command::new("powershell")
            .args([
                "-Command",
                &format!(
                    "Copy-Item -Path '{}' -Destination '{}' -Force",
                    image.display(),
                    sd_path
                ),
            ])
            .output()
            .await?;
    }

    Ok(())
}

/// Configure la partition boot avec SSH, WiFi, et hostname
async fn configure_boot_partition(config: &FlashConfig, ssh_public_key: &str) -> Result<()> {
    // Trouver la partition boot montée
    #[cfg(target_os = "macos")]
    let boot_path = Path::new("/Volumes/bootfs");

    #[cfg(target_os = "windows")]
    let boot_path = Path::new("E:\\"); // À ajuster dynamiquement

    #[cfg(target_os = "linux")]
    let boot_path = Path::new("/media/$USER/bootfs");

    // Attendre que la partition soit montée
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    // 1. Activer SSH (créer fichier vide)
    fs::write(boot_path.join("ssh"), "")?;

    // 2. Configurer le WiFi
    let wpa_supplicant = format!(
        r#"country=FR
ctrl_interface=DIR=/var/run/wpa_supplicant GROUP=netdev
update_config=1

network={{
    ssid="{}"
    psk="{}"
    key_mgmt=WPA-PSK
}}
"#,
        config.wifi_ssid, config.wifi_password
    );
    fs::write(boot_path.join("wpa_supplicant.conf"), wpa_supplicant)?;

    // 3. Configurer le hostname et user avec userconf.txt
    // Format: username:encrypted_password
    let user_conf = "maison:$6$rounds=656000$random$hash"; // TODO: générer proprement
    fs::write(boot_path.join("userconf.txt"), user_conf)?;

    // 4. Ajouter la clé SSH publique via firstrun.sh
    let firstrun_script = format!(
        r#"#!/bin/bash
set -e

# Configurer le hostname
raspi-config nonint do_hostname {}

# Configurer le timezone
timedatectl set-timezone {}

# Créer le dossier SSH et ajouter la clé
mkdir -p /home/maison/.ssh
echo '{}' > /home/maison/.ssh/authorized_keys
chown -R maison:maison /home/maison/.ssh
chmod 700 /home/maison/.ssh
chmod 600 /home/maison/.ssh/authorized_keys

# Désactiver le mot de passe SSH (clé uniquement)
sed -i 's/#PasswordAuthentication yes/PasswordAuthentication no/' /etc/ssh/sshd_config
systemctl restart ssh

# Supprimer ce script après exécution
rm -f /boot/firstrun.sh
sed -i 's| systemd.run.*||g' /boot/cmdline.txt

exit 0
"#,
        config.hostname, config.timezone, ssh_public_key
    );
    fs::write(boot_path.join("firstrun.sh"), firstrun_script)?;

    // 5. Modifier cmdline.txt pour exécuter firstrun.sh au boot
    let cmdline_path = boot_path.join("cmdline.txt");
    let mut cmdline = fs::read_to_string(&cmdline_path)?;
    cmdline = cmdline.trim().to_string();
    cmdline.push_str(" systemd.run=/boot/firstrun.sh systemd.run_success_action=reboot systemd.unit=kernel-command-line.target");
    fs::write(cmdline_path, cmdline)?;

    Ok(())
}

/// Exécute l'installation complète sur le Pi via SSH
pub async fn run_full_installation(
    window: Window,
    host: &str,
    username: &str,
    private_key: &str,
    config: InstallConfig,
) -> Result<()> {
    use crate::ssh;

    let steps = vec![
        ("update", "Mise à jour système", "sudo apt update && sudo apt upgrade -y"),
        ("docker", "Installation Docker", "curl -fsSL https://get.docker.com | sh && sudo usermod -aG docker $USER"),
        ("reboot", "Redémarrage", "sudo reboot"),
        ("clone", "Clonage configuration", "git clone https://github.com/nicolascleton/media-stack-fleet.git ~/media-stack-fleet"),
        ("structure", "Création structure", "mkdir -p ~/media-stack/{decypharr,jellyfin,radarr,sonarr,prowlarr,jellyseerr,bazarr} && sudo mkdir -p /mnt/decypharr && sudo chown $USER:$USER /mnt/decypharr"),
        ("compose", "Démarrage services", "cd ~/media-stack && docker compose up -d"),
    ];

    let total = steps.len() as u32;

    for (i, (id, name, cmd)) in steps.iter().enumerate() {
        let percent = (i as u32 * 100) / total;
        emit_progress(&window, id, percent, name, None);

        // Cas spécial pour le reboot
        if *id == "reboot" {
            ssh::execute_command(host, username, private_key, cmd).await.ok();
            // Attendre que le Pi redémarre
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            continue;
        }

        let output = ssh::execute_command(host, username, private_key, cmd).await?;
        tracing::info!("Step {}: {}", id, output);
    }

    // Configuration des services (via API)
    emit_progress(&window, "config", 80, "Configuration des services...", None);

    // TODO: Exécuter les appels API curl pour configurer Radarr, Sonarr, etc.
    // Utiliser les commandes du skill /flotte

    emit_progress(&window, "complete", 100, "Installation terminée !", None);

    Ok(())
}

/// Émet un événement de progression vers le frontend
fn emit_progress(window: &Window, step: &str, percent: u32, message: &str, speed: Option<&str>) {
    let _ = window.emit(
        "flash-progress",
        FlashProgress {
            step: step.to_string(),
            percent,
            message: message.to_string(),
            speed: speed.map(String::from),
        },
    );
}
