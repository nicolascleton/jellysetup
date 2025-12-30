use crate::SDCard;
use anyhow::Result;
use sysinfo::{Disks, System};

/// Liste les disques amovibles (cartes SD)
pub async fn list_removable_drives() -> Result<Vec<SDCard>> {
    let disks = Disks::new_with_refreshed_list();
    let mut sd_cards = Vec::new();

    for disk in disks.list() {
        // Filtrer pour ne garder que les disques amovibles
        if disk.is_removable() {
            let path = disk.mount_point().to_string_lossy().to_string();
            let name = disk.name().to_string_lossy().to_string();
            let size = disk.total_space();

            // Vérifier que c'est une carte SD (pas un disque système)
            #[cfg(target_os = "macos")]
            let is_sd = path.starts_with("/Volumes/") && !path.contains("Macintosh");

            #[cfg(target_os = "windows")]
            let is_sd = path.len() == 3; // "D:\" format

            #[cfg(target_os = "linux")]
            let is_sd = path.starts_with("/media/") || path.starts_with("/mnt/");

            if is_sd {
                sd_cards.push(SDCard {
                    path: get_raw_device_path(&path),
                    name: if name.is_empty() {
                        "Carte SD".to_string()
                    } else {
                        name
                    },
                    size,
                    removable: true,
                });
            }
        }
    }

    Ok(sd_cards)
}

/// Convertit le chemin de montage en chemin raw device
fn get_raw_device_path(mount_path: &str) -> String {
    #[cfg(target_os = "macos")]
    {
        // Sur macOS, on utilise diskutil pour trouver le device
        // /Volumes/SDCARD -> /dev/disk4
        use std::process::Command;

        let output = Command::new("diskutil")
            .args(["info", "-plist", mount_path])
            .output();

        if let Ok(output) = output {
            let stdout = String::from_utf8_lossy(&output.stdout);
            // Parser le plist pour extraire DeviceIdentifier
            if let Some(start) = stdout.find("<key>DeviceIdentifier</key>") {
                let rest = &stdout[start..];
                if let Some(value_start) = rest.find("<string>") {
                    if let Some(value_end) = rest[value_start..].find("</string>") {
                        let device = &rest[value_start + 8..value_start + value_end];
                        return format!("/dev/r{}", device); // raw device
                    }
                }
            }
        }
        mount_path.to_string()
    }

    #[cfg(target_os = "windows")]
    {
        // Sur Windows, convertir D: en \\.\PhysicalDrive#
        // Nécessite des appels Windows API
        use std::process::Command;

        let drive_letter = mount_path.chars().next().unwrap_or('D');
        let output = Command::new("wmic")
            .args([
                "diskdrive",
                "where",
                &format!("DeviceID like '%PhysicalDrive%'"),
                "get",
                "DeviceID,MediaType",
            ])
            .output();

        // Pour simplifier, on retourne le chemin direct
        format!("\\\\.\\{}:", drive_letter)
    }

    #[cfg(target_os = "linux")]
    {
        // Sur Linux, utiliser /dev/sdX
        use std::process::Command;

        let output = Command::new("findmnt")
            .args(["-n", "-o", "SOURCE", mount_path])
            .output();

        if let Ok(output) = output {
            let device = String::from_utf8_lossy(&output.stdout).trim().to_string();
            // Retirer le numéro de partition: /dev/sda1 -> /dev/sda
            if let Some(pos) = device.rfind(char::is_numeric) {
                return device[..pos].to_string();
            }
            return device;
        }
        mount_path.to_string()
    }
}

/// Démonte un disque avant le flash
pub async fn unmount_disk(device_path: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        Command::new("diskutil")
            .args(["unmountDisk", device_path])
            .output()?;
    }

    #[cfg(target_os = "windows")]
    {
        // Windows: utiliser mountvol ou PowerShell
        use std::process::Command;
        Command::new("mountvol")
            .args([device_path, "/D"])
            .output()?;
    }

    #[cfg(target_os = "linux")]
    {
        use std::process::Command;
        Command::new("umount").arg(device_path).output()?;
    }

    Ok(())
}

/// Éjecte un disque après le flash
pub async fn eject_disk(device_path: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        Command::new("diskutil")
            .args(["eject", device_path])
            .output()?;
    }

    #[cfg(target_os = "windows")]
    {
        // Windows: utiliser PowerShell pour éjecter
        use std::process::Command;
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
        use std::process::Command;
        Command::new("eject").arg(device_path).output()?;
    }

    Ok(())
}
