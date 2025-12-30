use crate::{FlashConfig, FlashProgress, InstallConfig};
use anyhow::{anyhow, Result};
use regex::Regex;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::Window;
use tokio::process::Command;

#[cfg(target_os = "macos")]
extern crate libc;

#[cfg(target_os = "macos")]
use std::os::unix::fs::PermissionsExt;

// Protection contre les lancements multiples
static FLASH_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

/// Guard RAII pour libérer le lock automatiquement
struct FlashGuard;

impl Drop for FlashGuard {
    fn drop(&mut self) {
        FLASH_IN_PROGRESS.store(false, Ordering::SeqCst);
        println!("[FLASH] Lock released - flash complete or failed");
    }
}

// URL de base pour lister les versions de Raspberry Pi OS
const RPI_OS_INDEX_URL: &str = "https://downloads.raspberrypi.com/raspios_lite_arm64/images/";

/// Récupère l'URL de la dernière version de Raspberry Pi OS Lite 64-bit
async fn get_latest_rpi_os_url() -> Result<(String, String)> {
    let client = reqwest::Client::new();

    // Récupérer la liste des versions
    let index_html = client.get(RPI_OS_INDEX_URL)
        .send()
        .await?
        .text()
        .await?;

    // Trouver la dernière version (format: raspios_lite_arm64-YYYY-MM-DD/)
    let re = Regex::new(r#"href="(raspios_lite_arm64-(\d{4}-\d{2}-\d{2})/)""#)?;

    let mut versions: Vec<(String, String)> = re.captures_iter(&index_html)
        .map(|cap| (cap[1].to_string(), cap[2].to_string()))
        .collect();

    // Trier par date décroissante
    versions.sort_by(|a, b| b.1.cmp(&a.1));

    let latest_folder = versions.first()
        .ok_or_else(|| anyhow!("Aucune version trouvée sur le serveur Raspberry Pi"))?;

    // Récupérer le contenu du dossier pour trouver le fichier .img.xz
    let folder_url = format!("{}{}", RPI_OS_INDEX_URL, latest_folder.0);
    let folder_html = client.get(&folder_url)
        .send()
        .await?
        .text()
        .await?;

    // Trouver le fichier .img.xz
    let file_re = Regex::new(r#"href="([^"]+\.img\.xz)""#)?;
    let image_filename = file_re.captures(&folder_html)
        .ok_or_else(|| anyhow!("Fichier image non trouvé"))?[1]
        .to_string();

    let full_url = format!("{}{}", folder_url, image_filename);
    let extracted_name = image_filename.trim_end_matches(".xz").to_string();

    tracing::info!("Dernière version Raspberry Pi OS: {}", latest_folder.1);
    tracing::info!("URL: {}", full_url);

    Ok((full_url, extracted_name))
}

/// Récupère la taille d'un disque en bytes
async fn get_disk_size(device_path: &str) -> Result<u64> {
    #[cfg(target_os = "macos")]
    {
        let disk_path = device_path.replace("/dev/r", "/dev/");
        let output = Command::new("diskutil")
            .args(["info", &disk_path])
            .output()
            .await?;

        let stdout = String::from_utf8_lossy(&output.stdout);

        // Chercher "Disk Size:" dans la sortie
        for line in stdout.lines() {
            if line.contains("Disk Size:") || line.contains("Total Size:") {
                // Format: "Disk Size:                 31.9 GB (31914983424 Bytes)"
                if let Some(start) = line.find('(') {
                    if let Some(end) = line.find(" Bytes)") {
                        let size_str = &line[start + 1..end];
                        if let Ok(size) = size_str.parse::<u64>() {
                            return Ok(size);
                        }
                    }
                }
            }
        }
        Err(anyhow!("Impossible de déterminer la taille du disque"))
    }

    #[cfg(not(target_os = "macos"))]
    {
        // Pour les autres OS, retourner une valeur par défaut sûre
        Ok(32 * 1024 * 1024 * 1024) // 32GB par défaut
    }
}

/// Flash Raspberry Pi OS sur la carte SD
pub async fn flash_raspberry_pi_os(
    window: Window,
    config: FlashConfig,
    ssh_public_key: String,
) -> Result<()> {
    println!("========================================");
    println!("[FLASH] Starting flash_raspberry_pi_os");
    println!("[FLASH] SD Path: {}", config.sd_path);
    println!("[FLASH] Hostname: {}", config.hostname);
    println!("========================================");

    // Protection contre les lancements multiples
    if FLASH_IN_PROGRESS.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
        println!("[FLASH] ERROR: Flash already in progress!");
        return Err(anyhow!("Un flash est déjà en cours. Veuillez patienter."));
    }
    println!("[FLASH] Lock acquired - no other flash can start");

    // Garantir qu'on libère le lock même en cas d'erreur
    let _guard = FlashGuard;

    let cache_dir = dirs::cache_dir()
        .ok_or_else(|| anyhow!("Cannot find cache directory"))?
        .join("jellysetup");

    println!("[FLASH] Cache dir: {:?}", cache_dir);

    fs::create_dir_all(&cache_dir).map_err(|e| {
        println!("[FLASH] ERROR creating cache dir: {:?}", e);
        anyhow!("Erreur création cache: {}", e)
    })?;
    println!("[FLASH] Cache dir created OK");

    // Étape 1: Récupérer la dernière version de Raspberry Pi OS
    // Étapes: Téléchargement (0-25%), Écriture (25-75%), Configuration (75-90%), Éjection (90-100%)
    emit_progress(&window, "download", 0, "Recherche de la dernière version...", None);
    println!("[FLASH] Getting latest RPI OS URL...");

    let (download_url, image_name) = get_latest_rpi_os_url().await.map_err(|e| {
        println!("[FLASH] ERROR getting RPI OS URL: {:?}", e);
        e
    })?;
    println!("[FLASH] URL: {}", download_url);
    println!("[FLASH] Image name: {}", image_name);

    let image_path = cache_dir.join(format!("{}.xz", &image_name));
    let extracted_path = cache_dir.join(&image_name);

    println!("[FLASH] Image path: {:?}", image_path);
    println!("[FLASH] Extracted path: {:?}", extracted_path);
    println!("[FLASH] Image exists: {}", image_path.exists());
    println!("[FLASH] Extracted exists: {}", extracted_path.exists());

    // Télécharger l'image si nécessaire
    emit_progress(&window, "download", 5, "Téléchargement en cours...", None);  // 0-20% pour download

    if !image_path.exists() {
        println!("[FLASH] Downloading image...");
        download_image(&window, &download_url, &image_path).await.map_err(|e| {
            println!("[FLASH] ERROR downloading: {:?}", e);
            e
        })?;
        println!("[FLASH] Download complete");
    } else {
        println!("[FLASH] Image already cached, skipping download");
    }

    emit_progress(&window, "download", 20, "Extraction de l'image...", None);  // Fin téléchargement

    // Étape 2: Extraire l'image XZ
    if !extracted_path.exists() {
        println!("[FLASH] Extracting image...");
        extract_xz(&image_path, &extracted_path).await.map_err(|e| {
            println!("[FLASH] ERROR extracting: {:?}", e);
            e
        })?;
        println!("[FLASH] Extraction complete");
    } else {
        println!("[FLASH] Image already extracted, skipping");
    }

    // Vérifier que le fichier extrait existe
    if !extracted_path.exists() {
        println!("[FLASH] ERROR: Extracted image not found at {:?}", extracted_path);
        return Err(anyhow!("Image extraite introuvable"));
    }

    let extracted_size = fs::metadata(&extracted_path).map(|m| m.len()).unwrap_or(0);
    println!("[FLASH] Extracted image size: {} bytes ({:.2} GB)", extracted_size, extracted_size as f64 / 1_000_000_000.0);

    // SÉCURITÉ: Vérification finale avant toute opération sur le disque
    emit_progress(&window, "download", 24, "Vérification de sécurité...", None);  // Presque fini téléchargement
    println!("[FLASH] Security verification...");

    // Récupérer la taille du disque sélectionné pour vérification
    let sd_size = get_disk_size(&config.sd_path).await.unwrap_or(0);
    println!("[FLASH] SD card size: {} bytes ({:.2} GB)", sd_size, sd_size as f64 / 1_000_000_000.0);

    crate::sd_card::verify_safe_to_flash(&config.sd_path, sd_size).map_err(|e| {
        println!("[FLASH] ERROR in verify_safe_to_flash: {:?}", e);
        e
    })?;
    println!("[FLASH] Security verification OK");

    emit_progress(&window, "download", 25, "Démontage de la carte SD...", None);  // Fin téléchargement = 25%
    println!("[FLASH] Unmounting disk...");

    // Étape 3: Démonter la carte SD
    crate::sd_card::unmount_disk(&config.sd_path).await.map_err(|e| {
        println!("[FLASH] ERROR unmounting: {:?}", e);
        e
    })?;
    println!("[FLASH] Unmount complete");

    emit_progress(&window, "write", 25, "Écriture de l'image...", None);  // Début écriture = 25%
    println!("[FLASH] ===== STARTING WRITE =====");
    println!("[FLASH] Source: {:?}", extracted_path);
    println!("[FLASH] Destination: {}", config.sd_path);

    // Étape 4: Écrire l'image sur la carte SD (APRÈS vérification de sécurité)
    write_image_to_sd(&window, &extracted_path, &config.sd_path).await.map_err(|e| {
        println!("[FLASH] ERROR in write_image_to_sd: {:?}", e);
        e
    })?;
    println!("[FLASH] Write complete!");

    emit_progress(&window, "configure", 75, "Configuration du système...", None);  // Configuration = 75-90%
    println!("[FLASH] Configuring boot partition...");

    // Étape 5: Configurer le boot (SSH, WiFi, hostname)
    configure_boot_partition(&config, &ssh_public_key).await.map_err(|e| {
        println!("[FLASH] ERROR configuring boot: {:?}", e);
        e
    })?;
    println!("[FLASH] Boot configured");

    emit_progress(&window, "eject", 90, "Éjection de la carte...", None);  // Éjection = 90-100%
    println!("[FLASH] Ejecting disk...");

    // Étape 6: Éjecter
    crate::sd_card::eject_disk(&config.sd_path).await.map_err(|e| {
        println!("[FLASH] ERROR ejecting: {:?}", e);
        e
    })?;
    println!("[FLASH] Eject complete");

    emit_progress(&window, "complete", 100, "Carte SD prête !", None);
    println!("========================================");
    println!("[FLASH] FLASH COMPLETE SUCCESS!");
    println!("========================================");

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

/// Écrit l'image sur la carte SD avec privilèges admin
async fn write_image_to_sd(_window: &Window, image: &Path, sd_path: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        let disk_id = sd_path
            .trim_start_matches("/dev/r")
            .trim_start_matches("/dev/");

        println!("[Flash] Writing image to {} (disk: {})", sd_path, disk_id);
        println!("[Flash] Image: {}", image.display());

        // Taille de l'image pour calculer la progression
        let image_size = std::fs::metadata(&image)?.len();
        println!("[Flash] Image size: {} bytes ({:.1} GB)", image_size, image_size as f64 / 1_000_000_000.0);

        // Utiliser le dossier cache pour le log (évite problèmes de permissions /tmp)
        let cache_dir = dirs::cache_dir()
            .ok_or_else(|| anyhow!("Cannot find cache directory"))?
            .join("jellysetup");
        let log_path = cache_dir.join("flash.log");
        let log_path_str = log_path.to_str().unwrap_or("/tmp/jellysetup_flash.log");

        println!("[Flash] Log path: {}", log_path_str);

        // Écrire un log initial
        match std::fs::write(&log_path, format!(
            "Starting dd...\nInput: {}\nOutput: {}\n",
            image.display(),
            sd_path
        )) {
            Ok(_) => println!("[Flash] Initial log written OK"),
            Err(e) => {
                println!("[Flash] ERROR writing initial log: {:?}", e);
                // On continue quand même, le log n'est pas critique
            }
        }

        println!("[Flash] Using dd + authopen method...");
        println!("[Flash] This will show a macOS authorization dialog");

        // Méthode qui fonctionne : dd pipe vers authopen
        // authopen gère l'autorisation et écrit sur le disque brut
        // dd if=IMAGE bs=1m | /usr/libexec/authopen -w /dev/rdiskN

        let mut child = std::process::Command::new("sh")
            .args([
                "-c",
                &format!(
                    "dd if=\"{}\" bs=1m 2>\"{}\" | /usr/libexec/authopen -w \"{}\"",
                    image.display(),
                    log_path_str,
                    sd_path
                )
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| {
                println!("[Flash] ERROR spawning dd|authopen: {:?}", e);
                anyhow!("Impossible de lancer le flash: {}", e)
            })?;

        println!("[Flash] dd|authopen spawned, PID: {}", child.id());

        // Écrire le début du log
        let _ = std::fs::write(&log_path, "=== Flash started ===\n");

        // Note: authopen va afficher un dialogue de mot de passe
        // Le processus va bloquer jusqu'à ce que l'utilisateur entre son mdp
        println!("[Flash] Flash process started, waiting for authorization dialog...");

        let child_pid = child.id();
        println!("[Flash] PID: {}", child_pid);

        // Monitorer la progression en lisant le log de dd
        let start_time = std::time::Instant::now();
        let mut last_percent = 0u32;
        let mut current_speed: f64 = 6.5; // Vitesse initiale estimée en MB/s
        let mut iteration = 0u32;

        loop {
            iteration += 1;
            if iteration % 10 == 1 {
                println!("[Flash] Loop iteration {}, elapsed: {}s", iteration, start_time.elapsed().as_secs());
            }

            // Vérifier si le processus est terminé
            match child.try_wait() {
                Ok(Some(status)) => {
                    println!("[Flash] =============================================");
                    println!("[Flash] Process finished with status: {:?}", status);
                    println!("[Flash] Exit code: {:?}", status.code());
                    println!("[Flash] Success: {}", status.success());

                    // Lire stdout et stderr de osascript
                    if let Some(mut stdout) = child.stdout.take() {
                        let mut stdout_str = String::new();
                        use std::io::Read;
                        let _ = stdout.read_to_string(&mut stdout_str);
                        println!("[Flash] Osascript STDOUT: '{}'", stdout_str);
                    }
                    if let Some(mut stderr) = child.stderr.take() {
                        let mut stderr_str = String::new();
                        use std::io::Read;
                        let _ = stderr.read_to_string(&mut stderr_str);
                        println!("[Flash] Osascript STDERR: '{}'", stderr_str);
                    }

                    // Lire le log final
                    println!("[Flash] Reading log file: {:?}", log_path);
                    match std::fs::read_to_string(&log_path) {
                        Ok(log_content) => {
                            println!("[Flash] Log file content ({} bytes):", log_content.len());
                            println!("----------------------------------------");
                            println!("{}", log_content);
                            println!("----------------------------------------");

                            // Vérifier si dd a réussi (méthode authopen)
                            // Le log contient la sortie stderr de dd: "XXXX bytes transferred"
                            if log_content.contains("bytes transferred") && status.success() {
                                println!("[Flash] SUCCESS: dd completed!");
                                // Sync pour s'assurer que tout est écrit
                                let _ = std::process::Command::new("sync").output();
                                break;
                            } else if log_content.contains("Operation not permitted") || log_content.contains("Permission denied") {
                                println!("[Flash] FAILED: Permission denied in log");
                                return Err(anyhow!(
                                    "macOS bloque l'écriture sur le disque.\n\n\
                                    Va dans Réglages Système > Confidentialité > Accès complet au disque\n\
                                    Ajoute JellySetup, puis quitte et relance l'app."
                                ));
                            } else if !status.success() {
                                println!("[Flash] FAILED: dd/authopen exit code non-zero");
                                return Err(anyhow!(
                                    "Erreur lors du flash. Log:\n{}", log_content
                                ));
                            }
                        }
                        Err(e) => {
                            println!("[Flash] ERROR reading log file: {:?}", e);
                        }
                    }

                    if !status.success() {
                        println!("[Flash] FAILED: Flash process returned non-success status");
                        return Err(anyhow!(
                            "Le flash a échoué (code: {:?}). L'utilisateur a peut-être annulé le dialogue de mot de passe.",
                            status.code()
                        ));
                    }
                    break;
                }
                Ok(None) => {
                    // Processus toujours en cours - envoyer SIGINFO pour obtenir la progression
                    let elapsed = start_time.elapsed().as_secs();
                    let mut total_written: u64 = 0;

                    // Envoyer SIGINFO au process dd pour qu'il écrive sa progression
                    if let Ok(output) = std::process::Command::new("pgrep")
                        .args(["-f", "dd if=.*raspios"])
                        .output()
                    {
                        if let Ok(pid_str) = String::from_utf8(output.stdout) {
                            if let Ok(pid) = pid_str.trim().parse::<i32>() {
                                unsafe { libc::kill(pid, libc::SIGINFO); }
                            }
                        }
                    }

                    // Attendre un peu que dd écrive dans le log
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

                    // Lire les dernières lignes du log dd
                    // Format SIGINFO: "2841640960 bytes transferred in 997.746971 secs (2848058 bytes/sec)"
                    if let Ok(log_content) = std::fs::read_to_string(&log_path) {
                        // Chercher la dernière ligne avec des bytes
                        for line in log_content.lines().rev() {
                            if line.contains("bytes") && line.contains("transferred") {
                                // Parser: "2841640960 bytes transferred..."
                                if let Some(bytes_str) = line.split_whitespace().next() {
                                    if let Ok(bytes) = bytes_str.parse::<u64>() {
                                        total_written = bytes;
                                    }
                                }
                                // Parser vitesse: "... (2848058 bytes/sec)"
                                if let Some(start) = line.rfind('(') {
                                    if let Some(end) = line.rfind(" bytes/sec)") {
                                        let speed_str = &line[start+1..end];
                                        if let Ok(bytes_per_sec) = speed_str.parse::<f64>() {
                                            current_speed = bytes_per_sec / 1_000_000.0; // Convertir en MB/s
                                        }
                                    }
                                }
                                break;
                            }
                        }
                    }

                    // Si pas de log, estimer avec le temps
                    if total_written == 0 {
                        total_written = elapsed * (current_speed as u64 * 1_000_000);
                    }

                    // Calculer le pourcentage RÉEL (pas de plafond artificiel)
                    let percent = ((total_written as f64 / image_size as f64) * 100.0).min(99.0) as u32;

                    // Calculer le temps restant estimé
                    let remaining_bytes = image_size.saturating_sub(total_written);
                    let remaining_secs = if current_speed > 0.1 {
                        (remaining_bytes as f64 / (current_speed * 1_000_000.0)) as u64
                    } else {
                        0
                    };
                    let remaining_min = remaining_secs / 60;
                    let remaining_sec = remaining_secs % 60;

                    // Émettre la progression
                    if percent > last_percent || elapsed % 3 == 0 {
                        last_percent = percent;
                        // Calculer progression totale: écriture = 25% à 75% (50% de la barre)
                        let total_percent = 25 + (percent * 50 / 100);
                        let time_str = if remaining_min > 0 {
                            format!("~{}min{}s restant", remaining_min, remaining_sec)
                        } else if remaining_secs > 0 {
                            format!("~{}s restant", remaining_secs)
                        } else {
                            "finalisation...".to_string()
                        };
                        let speed_display = format!("{:.1} MB/s", current_speed);
                        emit_progress(_window, "write", total_percent,
                            &format!("Écriture: {}% - {}", percent, time_str), Some(&speed_display));

                        println!("[Flash] Progress: {}% - Speed: {:.1} MB/s - Written: {:.1} GB",
                            percent, current_speed, total_written as f64 / 1_000_000_000.0);
                    }

                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                }
                Err(e) => {
                    return Err(anyhow!("Erreur lors du monitoring: {}", e));
                }
            }
        }

        // Sync pour s'assurer que tout est écrit
        emit_progress(_window, "write", 74, "Synchronisation...", None);  // Fin écriture = ~75%
        let _ = Command::new("sync").output().await;

        println!("[Flash] Write completed successfully!");
    }

    #[cfg(target_os = "linux")]
    {
        // Sur Linux, utiliser pkexec pour l'authentification graphique
        let output = Command::new("pkexec")
            .args([
                "dd",
                &format!("if={}", image.display()),
                &format!("of={}", sd_path),
                "bs=4M",
                "status=progress",
            ])
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("Erreur d'écriture: {}", stderr));
        }
    }

    #[cfg(target_os = "windows")]
    {
        // Sur Windows, utiliser PowerShell avec élévation
        Command::new("powershell")
            .args([
                "-Command",
                &format!(
                    "Start-Process -Verb RunAs -Wait -FilePath 'cmd' -ArgumentList '/c dd if=\"{}\" of=\"{}\" bs=4M'",
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
    #[cfg(target_os = "macos")]
    {
        // Extraire le disk identifier correctement (ex: /dev/rdisk11 -> disk11)
        let disk_id = config.sd_path
            .trim_start_matches("/dev/r")
            .trim_start_matches("/dev/");

        println!("[Config] Forcing partition table reload for: {}", disk_id);

        // Méthode: utiliser diskutil repairDisk pour forcer la relecture de la table de partition
        // Cela nécessite des privilèges admin
        let script = format!(
            r#"do shell script "diskutil unmountDisk force {} && sleep 2 && diskutil mountDisk {}" with administrator privileges"#,
            disk_id, disk_id
        );

        println!("[Config] Running remount with admin privileges...");
        let output = Command::new("osascript")
            .args(["-e", &script])
            .output()
            .await?;

        println!("[Config] Remount stdout: {}", String::from_utf8_lossy(&output.stdout));
        if !output.status.success() {
            println!("[Config] Remount stderr: {}", String::from_utf8_lossy(&output.stderr));
        }

        // Attendre que les partitions apparaissent
        println!("[Config] Waiting for partitions to appear...");
        for i in 0..10 {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;

            if Path::new("/Volumes/bootfs").exists() {
                println!("[Config] bootfs found after {}s", i + 1);
                break;
            }
            println!("[Config] Waiting... ({}s)", i + 1);
        }
    }

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Trouver la partition boot montée
    #[cfg(target_os = "macos")]
    let boot_path = {
        // Lister les volumes disponibles pour debug
        if let Ok(entries) = std::fs::read_dir("/Volumes") {
            println!("[Config] Available volumes:");
            for entry in entries.flatten() {
                println!("[Config]   - {}", entry.path().display());
            }
        }

        // Chercher la partition boot (plusieurs noms possibles)
        let possible_names = ["bootfs", "boot", "BOOTFS", "BOOT", "RPI-RP2", "NO NAME"];

        let mut found_path: Option<&Path> = None;
        for name in &possible_names {
            let path_str = format!("/Volumes/{}", name);
            let path = Path::new(&path_str);
            if path.exists() {
                println!("[Config] Found boot partition at: {}", path_str);
                found_path = Some(Box::leak(path_str.into_boxed_str()) as &str).map(Path::new);
                break;
            }
        }

        match found_path {
            Some(p) => p,
            None => return Err(anyhow!(
                "Partition boot non trouvée.\n\n\
                Volumes disponibles dans /Volumes mais aucun ne correspond à bootfs/boot.\n\
                Essayez de débrancher et rebrancher la carte SD."
            )),
        }
    };

    #[cfg(target_os = "windows")]
    let boot_path = Path::new("E:\\"); // À ajuster dynamiquement

    #[cfg(target_os = "linux")]
    let boot_path = Path::new("/media/$USER/bootfs");

    // 1. Activer SSH (créer fichier vide - backup pour compatibilité)
    fs::write(boot_path.join("ssh"), "")?;
    println!("[Config] Created ssh file");

    // 2. Créer custom.toml (méthode Bookworm 2024+)
    // Ce fichier est lu par raspberrypi-sys-mods au premier boot
    let custom_toml = format!(
        r#"# Configuration JellySetup - Raspberry Pi OS Bookworm
config_version = 1

[system]
hostname = "{hostname}"

[user]
name = "{username}"
password = "{password}"
password_encrypted = false

[ssh]
enabled = true
password_authentication = true
authorized_keys = [ "{ssh_key}" ]

[wlan]
ssid = "{wifi_ssid}"
password = "{wifi_password}"
password_encrypted = false
hidden = false
country = "{wifi_country}"

[locale]
keymap = "{keymap}"
timezone = "{timezone}"
"#,
        hostname = config.hostname,
        username = config.system_username,
        password = config.system_password,
        ssh_key = ssh_public_key,
        wifi_ssid = config.wifi_ssid,
        wifi_password = config.wifi_password,
        wifi_country = config.wifi_country,
        keymap = config.keymap,
        timezone = config.timezone,
    );
    fs::write(boot_path.join("custom.toml"), custom_toml)?;
    println!("[Config] Created custom.toml with hostname={}, user={}", config.hostname, config.system_username);

    // 3. Créer aussi userconf.txt en backup (pour anciennes versions)
    // Format: username:password (non chiffré pour simplifier, custom.toml prendra le relais)
    let userconf = format!("{}:{}", config.system_username, config.system_password);
    fs::write(boot_path.join("userconf.txt"), userconf)?;
    println!("[Config] Created userconf.txt backup");

    Ok(())
}

/// Génère le contenu du docker-compose.yml avec tous les services
fn generate_docker_compose(hostname: &str, cloudflare_token: Option<&str>) -> String {
    let supabase_url = crate::supabase::get_supabase_url_public();
    let supabase_service_key = crate::supabase::get_supabase_service_key();

    let mut compose = format!(r#"---
# =============================================================================
# Docker Compose - Media Stack
# Généré par JellySetup
# Pi: {hostname}
# =============================================================================

services:
  # Decypharr - Gestionnaire AllDebrid + montage WebDAV/Rclone
  decypharr:
    image: cy01/blackhole:latest
    container_name: decypharr
    restart: always
    cap_add:
      - SYS_ADMIN
    security_opt:
      - apparmor:unconfined
    ports:
      - 8282:8282
    volumes:
      - /mnt:/mnt:rshared
      - /mnt/decypharr/qbit:/mnt/decypharr/qbit
      - ./decypharr:/app
    environment:
      - TZ=Europe/Paris
      - PUID=1000
      - PGID=1000
    devices:
      - /dev/fuse:/dev/fuse:rwm

  # Jellyfin - Serveur multimédia principal
  jellyfin:
    image: lscr.io/linuxserver/jellyfin:latest
    container_name: jellyfin
    restart: unless-stopped
    ports:
      - 8096:8096
    environment:
      - TZ=Europe/Paris
      - PUID=1000
      - PGID=1000
      - JELLYFIN_FFmpeg__probesize=1G
      - JELLYFIN_FFmpeg__analyzeduration=200M
    volumes:
      - ./jellyfin:/config
      - /mnt:/mnt:rshared
    devices:
      - /dev/dri:/dev/dri
    deploy:
      resources:
        limits:
          memory: 4G
        reservations:
          memory: 1G

  # Radarr - Gestionnaire de films
  radarr:
    image: lscr.io/linuxserver/radarr:latest
    container_name: radarr
    restart: unless-stopped
    ports:
      - 7878:7878
    volumes:
      - ./radarr:/config
      - /mnt:/mnt:rslave
    environment:
      - TZ=Europe/Paris
      - PUID=1000
      - PGID=1000

  # Sonarr - Gestionnaire de séries
  sonarr:
    image: lscr.io/linuxserver/sonarr:latest
    container_name: sonarr
    restart: unless-stopped
    ports:
      - 8989:8989
    volumes:
      - ./sonarr:/config
      - /mnt:/mnt:rslave
    environment:
      - TZ=Europe/Paris
      - PUID=1000
      - PGID=1000

  # Prowlarr - Gestionnaire d'indexeurs
  prowlarr:
    image: lscr.io/linuxserver/prowlarr:latest
    container_name: prowlarr
    restart: unless-stopped
    ports:
      - 9696:9696
    volumes:
      - ./prowlarr:/config
    environment:
      - TZ=Europe/Paris
      - PUID=1000
      - PGID=1000

  # Jellyseerr - Interface de requêtes
  jellyseerr:
    image: fallenbagel/jellyseerr:latest
    container_name: jellyseerr
    restart: unless-stopped
    ports:
      - 5055:5055
    volumes:
      - ./jellyseerr:/app/config
    environment:
      - TZ=Europe/Paris

  # Bazarr - Gestionnaire de sous-titres
  bazarr:
    image: lscr.io/linuxserver/bazarr:latest
    container_name: bazarr
    restart: unless-stopped
    ports:
      - 6767:6767
    environment:
      - TZ=Europe/Paris
      - PUID=1000
      - PGID=1000
    volumes:
      - ./bazarr:/config
      - /mnt:/mnt:rslave

  # FlareSolverr - Bypass Cloudflare pour les indexeurs
  flaresolverr:
    image: ghcr.io/flaresolverr/flaresolverr:latest
    container_name: flaresolverr
    restart: unless-stopped
    ports:
      - 8191:8191
    environment:
      - TZ=Europe/Paris
      - LOG_LEVEL=info

  # Supabazarr - Sauvegarde automatique vers Supabase
  # Interface web: http://<pi-ip>:8383
  supabazarr:
    image: ghcr.io/nicolascleton/supabazarr:latest
    container_name: supabazarr
    restart: unless-stopped
    ports:
      - 8383:8383
    environment:
      - TZ=Europe/Paris
      - PUID=1000
      - PGID=1000
      - SUPABASE_URL={supabase_url}
      - SUPABASE_SERVICE_KEY={supabase_service_key}
      - HOSTNAME={hostname}
      - MEDIA_STACK_PATH=/media-stack
      - BACKUP_HOUR=03:00
    volumes:
      - ./:/media-stack:ro
      - supabazarr_data:/etc/supabazarr
    deploy:
      resources:
        limits:
          memory: 128M
          cpus: '0.25'
    logging:
      driver: "json-file"
      options:
        max-size: "10m"
        max-file: "3"
"#);

    // Ajouter Cloudflared si token fourni
    if let Some(token) = cloudflare_token {
        if !token.is_empty() {
            compose.push_str(&format!(r#"
  # Cloudflared - Tunnel Cloudflare pour accès distant
  cloudflared:
    image: cloudflare/cloudflared:latest
    container_name: cloudflared
    restart: unless-stopped
    command: tunnel --no-autoupdate --protocol http2 run
    environment:
      - TUNNEL_TOKEN={token}
"#));
        }
    }

    // Ajouter les volumes et networks
    compose.push_str(r#"
volumes:
  supabazarr_data:

networks:
  default:
    name: media-network
"#);

    compose
}

/// Exécute l'installation complète sur le Pi via SSH
pub async fn run_full_installation(
    window: Window,
    host: &str,
    username: &str,
    private_key: &str,
    config: InstallConfig,
    hostname: &str,
) -> Result<()> {
    use crate::ssh;

    // Générer le docker-compose.yml avec tous les services
    let docker_compose = generate_docker_compose(
        hostname,
        config.cloudflare_token.as_deref()
    );

    // Étape 1: Mise à jour système
    emit_progress(&window, "update", 0, "Mise à jour système...", None);
    ssh::execute_command(host, username, private_key,
        "sudo apt update && sudo apt upgrade -y && sudo apt install -y git curl"
    ).await?;

    // Étape 2: Installation Docker
    emit_progress(&window, "docker", 15, "Installation Docker...", None);
    ssh::execute_command(host, username, private_key,
        "curl -fsSL https://get.docker.com | sh && sudo usermod -aG docker $USER"
    ).await?;

    // Étape 3: Redémarrage pour appliquer groupe docker
    emit_progress(&window, "reboot", 30, "Redémarrage...", None);
    ssh::execute_command(host, username, private_key, "sudo reboot").await.ok();
    tokio::time::sleep(std::time::Duration::from_secs(60)).await;

    // Attendre que le Pi soit de nouveau accessible
    for i in 0..30 {
        if ssh::execute_command(host, username, private_key, "echo ok").await.is_ok() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        if i == 29 {
            return Err(anyhow!("Pi not responding after reboot"));
        }
    }

    // Étape 4: Création de la structure
    emit_progress(&window, "structure", 40, "Création structure...", None);
    ssh::execute_command(host, username, private_key,
        "mkdir -p ~/media-stack/{decypharr,jellyfin,radarr,sonarr,prowlarr,jellyseerr,bazarr,logs} && \
         sudo mkdir -p /mnt/decypharr && \
         sudo chown $USER:$USER /mnt/decypharr"
    ).await?;

    // Étape 5: Écrire le docker-compose.yml
    emit_progress(&window, "compose_write", 50, "Génération docker-compose.yml...", None);
    let escaped_compose = docker_compose.replace("'", "'\\''");
    let write_cmd = format!("cat > ~/media-stack/docker-compose.yml << 'EOFCOMPOSE'\n{}\nEOFCOMPOSE", docker_compose);
    ssh::execute_command(host, username, private_key, &write_cmd).await?;

    // Étape 6: Démarrer les services
    emit_progress(&window, "compose_up", 60, "Démarrage des services Docker...", None);
    ssh::execute_command(host, username, private_key,
        "cd ~/media-stack && docker compose pull && docker compose up -d"
    ).await?;

    // Étape 7: Attendre que les services soient prêts
    emit_progress(&window, "wait_services", 75, "Attente des services...", None);
    tokio::time::sleep(std::time::Duration::from_secs(30)).await;

    // Étape 8: Configuration des services via API
    emit_progress(&window, "config", 85, "Configuration des services...", None);

    // TODO: Configurer Radarr, Sonarr, Prowlarr, Jellyfin via leurs APIs
    // - Ajouter root folders
    // - Configurer download clients
    // - Ajouter indexeurs YGG si passkey fourni
    // - Créer utilisateur Jellyfin

    emit_progress(&window, "complete", 100, "Installation terminée !", None);

    tracing::info!("Installation completed successfully on {}", host);
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
