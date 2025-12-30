use crate::SDCard;
use anyhow::{anyhow, Result};
use std::process::Command;

// Taille max pour une carte SD (512 GB) - sécurité pour ne pas formater un SSD
const MAX_SD_SIZE_BYTES: u64 = 512 * 1024 * 1024 * 1024;
// Taille min pour une carte SD utilisable (4 GB)
const MIN_SD_SIZE_BYTES: u64 = 4 * 1024 * 1024 * 1024;

/// Liste les cartes SD disponibles
pub async fn list_removable_drives() -> Result<Vec<SDCard>> {
    #[cfg(target_os = "macos")]
    {
        list_sd_cards_macos().await
    }

    #[cfg(target_os = "windows")]
    {
        list_sd_cards_windows().await
    }

    #[cfg(target_os = "linux")]
    {
        list_sd_cards_linux().await
    }
}

/// Liste les cartes SD sur macOS - approche ultra-simple
#[cfg(target_os = "macos")]
async fn list_sd_cards_macos() -> Result<Vec<SDCard>> {
    let mut sd_cards = Vec::new();

    // Lister tous les disques
    let output = Command::new("diskutil")
        .args(["list"])
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);

    println!("[SD Detection] diskutil list output:");
    println!("{}", stdout);

    // Chercher les disques physiques (pas synthesized)
    for line in stdout.lines() {
        // Format: /dev/disk11 (internal, physical):
        if line.starts_with("/dev/disk") && line.contains("physical") && !line.contains("synthesized") {
            // Extraire disk id: "/dev/disk11 (internal, physical):" -> "disk11"
            if let Some(disk_part) = line.split_whitespace().next() {
                let disk_id = disk_part.trim_start_matches("/dev/");

                println!("[SD Detection] Found physical disk: {}", disk_id);

                // Ignorer disk0-3 (disques système)
                if disk_id == "disk0" || disk_id == "disk1" || disk_id == "disk2" || disk_id == "disk3" {
                    println!("[SD Detection] Skipping system disk: {}", disk_id);
                    continue;
                }

                // Récupérer les infos du disque
                if let Some(sd) = get_disk_info(disk_id).await {
                    println!("[SD Detection] Valid SD card found: {} ({} GB)", sd.name, sd.size / 1024 / 1024 / 1024);
                    sd_cards.push(sd);
                } else {
                    println!("[SD Detection] Disk {} rejected after info check", disk_id);
                }
            }
        }
    }

    println!("[SD Detection] Total SD cards found: {}", sd_cards.len());
    Ok(sd_cards)
}

/// Récupère les infos d'un disque
#[cfg(target_os = "macos")]
async fn get_disk_info(disk_id: &str) -> Option<SDCard> {
    // D'abord récupérer la taille du disque entier
    let output = Command::new("diskutil")
        .args(["info", disk_id])
        .output()
        .ok()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut size: u64 = 0;

    for line in stdout.lines() {
        if line.contains("Disk Size:") || line.contains("Total Size:") {
            if let Some(start_idx) = line.find('(') {
                if let Some(end_idx) = line.find(" Bytes)") {
                    let bytes_str = &line[start_idx + 1..end_idx];
                    size = bytes_str.parse().unwrap_or(0);
                }
            }
        }
    }

    // Vérifier la taille
    if size < MIN_SD_SIZE_BYTES || size > MAX_SD_SIZE_BYTES {
        println!("[SD Detection] Disk {} size {} out of range", disk_id, size);
        return None;
    }

    // Chercher le nom du volume sur la première partition (disk11s1)
    let partition_id = format!("{}s1", disk_id);
    let volume_name = get_volume_name(&partition_id).unwrap_or_default();

    // Nom final: nom du volume si disponible, sinon disk_id
    let display_name = if volume_name.is_empty() || volume_name == "Not applicable (no file system)" {
        format!("{} - Carte SD", disk_id)
    } else {
        format!("{} ({})", volume_name, disk_id)
    };

    println!("[SD Detection] {} -> {} ({} GB)", disk_id, display_name, size / 1024 / 1024 / 1024);

    Some(SDCard {
        path: format!("/dev/r{}", disk_id),
        name: display_name,
        size,
        removable: true,
    })
}

/// Récupère le nom du volume d'une partition
#[cfg(target_os = "macos")]
fn get_volume_name(partition_id: &str) -> Option<String> {
    let output = Command::new("diskutil")
        .args(["info", partition_id])
        .output()
        .ok()?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    for line in stdout.lines() {
        if line.contains("Volume Name:") {
            if let Some(value) = line.split(':').last() {
                let name = value.trim().to_string();
                if !name.is_empty() && name != "Not applicable (no file system)" {
                    return Some(name);
                }
            }
        }
    }
    None
}

#[cfg(target_os = "windows")]
async fn list_sd_cards_windows() -> Result<Vec<SDCard>> {
    Ok(Vec::new())
}

#[cfg(target_os = "linux")]
async fn list_sd_cards_linux() -> Result<Vec<SDCard>> {
    Ok(Vec::new())
}

/// Vérifie une dernière fois avant le flash que c'est bien une carte SD
pub fn verify_safe_to_flash(device_path: &str, expected_size: u64) -> Result<()> {
    // Extraire le disk id du path (ex: /dev/rdisk11 -> disk11)
    let disk_id = device_path
        .trim_start_matches("/dev/r")
        .trim_start_matches("/dev/");

    // Vérifier que ce n'est pas un disque système (disk0, disk1, disk2, disk3)
    let is_system_disk = disk_id == "disk0" || disk_id == "disk1"
        || disk_id == "disk2" || disk_id == "disk3";

    if is_system_disk {
        return Err(anyhow!("SECURITE: Impossible de flasher le disque systeme!"));
    }

    if expected_size > MAX_SD_SIZE_BYTES {
        return Err(anyhow!("SECURITE: Disque trop grand pour etre une carte SD (max 512GB)"));
    }

    if expected_size < MIN_SD_SIZE_BYTES {
        return Err(anyhow!("SECURITE: Disque trop petit (min 4GB requis)"));
    }

    Ok(())
}

/// Démonte un disque avant le flash
pub async fn unmount_disk(device_path: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        // Convertir /dev/rdisk11 -> disk11
        let disk_id = device_path
            .trim_start_matches("/dev/r")
            .trim_start_matches("/dev/");

        println!("[SD] Unmounting disk: {}", disk_id);

        // Force unmount de toutes les partitions
        let output = Command::new("diskutil")
            .args(["unmountDisk", "force", disk_id])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            println!("[SD] Unmount warning: {}", stderr);
        }

        // Attendre un peu que le système libère le disque
        std::thread::sleep(std::time::Duration::from_secs(1));
    }

    #[cfg(target_os = "windows")]
    {
        Command::new("mountvol")
            .args([device_path, "/D"])
            .output()?;
    }

    #[cfg(target_os = "linux")]
    {
        Command::new("umount").arg(device_path).output()?;
    }

    Ok(())
}

/// Éjecte un disque après le flash
pub async fn eject_disk(device_path: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        Command::new("diskutil")
            .args(["eject", device_path])
            .output()?;
    }

    #[cfg(target_os = "windows")]
    {
        let script = format!(
            "$drive = Get-WmiObject Win32_Volume | Where-Object {{ $_.DeviceID -eq '{}' }}; $drive.Dismount($false, $false)",
            device_path.replace("\\", "\\\\")
        );
        Command::new("powershell")
            .args(["-Command", &script])
            .output()?;
    }

    #[cfg(target_os = "linux")]
    {
        Command::new("eject").arg(device_path).output()?;
    }

    Ok(())
}
