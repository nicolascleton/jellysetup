use crate::{FlashConfig, FlashProgress, InstallConfig, JellyfinAuth};
use anyhow::{anyhow, Result};
use regex::Regex;
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::Window;
use tokio::process::Command;

#[cfg(target_os = "macos")]
extern crate libc;

#[cfg(target_os = "macos")]
use std::os::unix::fs::PermissionsExt;

/// Debug logging - écrit dans /tmp/jellysetup_debug.log
fn debug_log(msg: &str) {
    println!("{}", msg);
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/jellysetup_debug.log")
    {
        let timestamp = chrono::Local::now().format("%H:%M:%S%.3f");
        let _ = writeln!(file, "[{}] {}", timestamp, msg);
    }
}

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

/// Récupère l'URL de la dernière version de Raspberry Pi OS Lite 64-bit (Bookworm)
/// Note: On évite Trixie car custom.toml ne fonctionne pas (cloud-init requis)
async fn get_latest_rpi_os_url() -> Result<(String, String)> {
    let client = reqwest::Client::new();

    // Récupérer la liste des versions
    let index_html = client.get(RPI_OS_INDEX_URL)
        .send()
        .await?
        .text()
        .await?;

    // Trouver toutes les versions (format: raspios_lite_arm64-YYYY-MM-DD/)
    let re = Regex::new(r#"href="(raspios_lite_arm64-(\d{4}-\d{2}-\d{2})/)""#)?;

    let mut versions: Vec<(String, String)> = re.captures_iter(&index_html)
        .map(|cap| (cap[1].to_string(), cap[2].to_string()))
        .collect();

    // Trier par date décroissante
    versions.sort_by(|a, b| b.1.cmp(&a.1));

    // Chercher la dernière version BOOKWORM (pas Trixie)
    // On vérifie le contenu de chaque dossier jusqu'à trouver une version bookworm
    let mut latest_folder: Option<&(String, String)> = None;
    let mut image_filename = String::new();

    for version in &versions {
        let folder_url = format!("{}{}", RPI_OS_INDEX_URL, version.0);
        if let Ok(resp) = client.get(&folder_url).send().await {
            if let Ok(folder_html) = resp.text().await {
                // Chercher un fichier bookworm (pas trixie)
                if folder_html.contains("bookworm") && !folder_html.contains("trixie") {
                    let file_re = Regex::new(r#"href="([^"]*bookworm[^"]*\.img\.xz)""#)?;
                    if let Some(cap) = file_re.captures(&folder_html) {
                        image_filename = cap[1].to_string();
                        latest_folder = Some(version);
                        println!("[Flash] Found Bookworm version: {}", version.0);
                        break;
                    }
                }
            }
        }
    }

    let latest_folder = latest_folder
        .ok_or_else(|| anyhow!("Aucune version Bookworm trouvée sur le serveur Raspberry Pi"))?;

    let folder_url = format!("{}{}", RPI_OS_INDEX_URL, latest_folder.0);

    // Si on n'a pas encore le nom du fichier, le récupérer
    let image_filename = if image_filename.is_empty() {
        let folder_html = client.get(&folder_url)
            .send()
            .await?
            .text()
            .await?;
        let file_re = Regex::new(r#"href="([^"]+\.img\.xz)""#)?;
        file_re.captures(&folder_html)
            .ok_or_else(|| anyhow!("Fichier image non trouvé"))?[1]
            .to_string()
    } else {
        image_filename
    };

    let full_url = format!("{}{}", folder_url, image_filename);
    let extracted_name = image_filename.trim_end_matches(".xz").to_string();

    println!("[Flash] Using Raspberry Pi OS Bookworm: {}", latest_folder.1);
    println!("[Flash] URL: {}", full_url);

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

    // Empêcher la mise en veille du Mac pendant le flash
    #[cfg(target_os = "macos")]
    let _caffeinate = {
        match std::process::Command::new("caffeinate")
            .args(["-dims"]) // display, idle, disk, system sleep prevention
            .spawn()
        {
            Ok(child) => {
                println!("[FLASH] caffeinate started (PID: {})", child.id());
                Some(child)
            }
            Err(e) => {
                println!("[FLASH] Warning: could not start caffeinate: {}", e);
                None
            }
        }
    };

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
async fn extract_xz(src: &Path, _dest: &Path) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        // Essayer plusieurs chemins pour xz (Homebrew ARM, Homebrew Intel, système)
        let xz_paths = ["/opt/homebrew/bin/xz", "/usr/local/bin/xz", "/usr/bin/xz", "xz"];
        let mut xz_cmd = None;

        for path in &xz_paths {
            if std::path::Path::new(path).exists() || *path == "xz" {
                xz_cmd = Some(*path);
                break;
            }
        }

        let xz_path = xz_cmd.ok_or_else(|| anyhow!("xz not found. Install with: brew install xz"))?;
        println!("[Extract] Using xz at: {}", xz_path);

        let output = Command::new(xz_path)
            .args(["-dk", src.to_str().unwrap()])
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            println!("[Extract] xz stderr: {}", stderr);
            return Err(anyhow!("xz extraction failed: {}", stderr));
        }
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
        let mut current_speed: f64 = 3.0; // Vitesse initiale estimée en MB/s (conservateur pour SD)
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
                    // Chercher le process dd avec le chemin de l'image (bookworm ou raspios)
                    if let Ok(output) = std::process::Command::new("pgrep")
                        .args(["-f", "dd if=.*/jellysetup/.*\\.img"])
                        .output()
                    {
                        if let Ok(pid_str) = String::from_utf8(output.stdout) {
                            for line in pid_str.lines() {
                                if let Ok(pid) = line.trim().parse::<i32>() {
                                    unsafe { libc::kill(pid, libc::SIGINFO); }
                                    break; // On envoie qu'au premier process trouvé
                                }
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
    depends_on:
      - jellyfin
    extra_hosts:
      - "host.docker.internal:host-gateway"

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
    healthcheck:
      test: ["CMD", "python", "-c", "import urllib.request; urllib.request.urlopen('http://localhost:8383/health')"]
      interval: 30s
      timeout: 10s
      retries: 3
      start_period: 10s
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
        "sudo DEBIAN_FRONTEND=noninteractive apt update && sudo DEBIAN_FRONTEND=noninteractive apt upgrade -y -o Dpkg::Options::='--force-confdef' -o Dpkg::Options::='--force-confold' && sudo apt install -y git curl"
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

    // 8.1: Attendre que Jellyfin soit prêt (max 2 min)
    emit_progress(&window, "config", 86, "Attente de Jellyfin...", None);
    let mut jellyfin_ready = false;
    for i in 0..24 {
        let check = ssh::execute_command(host, username, private_key,
            "curl -s -o /dev/null -w '%{http_code}' http://localhost:8096/health 2>/dev/null || echo 000"
        ).await.unwrap_or_default();
        if check.trim() == "200" {
            jellyfin_ready = true;
            println!("[Config] Jellyfin is ready");
            break;
        }
        println!("[Config] Waiting for Jellyfin ({}/24)...", i + 1);
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }

    if jellyfin_ready {
        // 8.2: Configurer Jellyfin via l'API Startup (compatible Jellyfin 10.11.x)
        emit_progress(&window, "config", 87, "Configuration Jellyfin...", None);

        let jf_user = config.jellyfin_username.replace("\\", "\\\\").replace("\"", "\\\"");
        let jf_pass = config.jellyfin_password.replace("\\", "\\\\").replace("\"", "\\\"");

        // Étape 1: Initialiser l'utilisateur (GET /Startup/FirstUser créé l'utilisateur par défaut)
        // En Jellyfin 10.11.x, il faut GET FirstUser avant de pouvoir POST User
        let first_user_cmd = "curl -s 'http://localhost:8096/Startup/FirstUser'";
        let first_user_result = ssh::execute_command(host, username, private_key, first_user_cmd).await.unwrap_or_default();
        println!("[Config] Jellyfin FirstUser: {}", first_user_result);

        // Petite pause pour laisser Jellyfin créer l'utilisateur
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;

        // Étape 2: Configuration initiale (langue, métadonnées)
        let startup_config_cmd = r#"curl -s -X POST 'http://localhost:8096/Startup/Configuration' \
            -H 'Content-Type: application/json' \
            -d '{"UICulture":"fr","MetadataCountryCode":"FR","PreferredMetadataLanguage":"fr"}'"#;
        ssh::execute_command(host, username, private_key, startup_config_cmd).await.ok();

        // Étape 3: Mettre à jour l'utilisateur admin (POST /Startup/User)
        let startup_user_cmd = format!(
            r#"curl -s -X POST 'http://localhost:8096/Startup/User' \
            -H 'Content-Type: application/json' \
            -d '{{"Name":"{}","Password":"{}"}}'  "#,
            jf_user, jf_pass
        );
        let user_result = ssh::execute_command(host, username, private_key, &startup_user_cmd).await;
        match &user_result {
            Ok(r) => println!("[Config] Jellyfin user updated: {}", r),
            Err(e) => println!("[Config] Jellyfin user update warning: {}", e),
        }

        // Étape 4: Activer l'accès distant
        let remote_access_cmd = r#"curl -s -X POST 'http://localhost:8096/Startup/RemoteAccess' \
            -H 'Content-Type: application/json' \
            -d '{"EnableRemoteAccess":true,"EnableAutomaticPortMapping":false}'"#;
        ssh::execute_command(host, username, private_key, remote_access_cmd).await.ok();

        // Étape 5: Compléter le wizard
        ssh::execute_command(host, username, private_key, "curl -s -X POST 'http://localhost:8096/Startup/Complete'").await.ok();
        println!("[Config] Jellyfin setup wizard completed");

        // S'authentifier pour créer les bibliothèques
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        let auth_cmd = format!(r#"curl -s -X POST 'http://localhost:8096/Users/AuthenticateByName' \
            -H 'Content-Type: application/json' \
            -H 'X-Emby-Authorization: MediaBrowser Client="JellySetup", Device="RaspberryPi", DeviceId="jellysetup-install", Version="1.0.0"' \
            -d '{{"Username":"{}","Pw":"{}"}}'  "#, jf_user, jf_pass);
        let auth_result = ssh::execute_command(host, username, private_key, &auth_cmd).await.unwrap_or_default();

        if let Some(token_start) = auth_result.find("\"AccessToken\":\"") {
            let token_rest = &auth_result[token_start + 15..];
            if let Some(token_end) = token_rest.find("\"") {
                let jellyfin_token = &token_rest[..token_end];
                println!("[Config] Jellyfin authenticated, creating libraries...");

                // Créer bibliothèque Films avec LibraryOptions.PathInfos (OBLIGATOIRE pour avoir un ItemId!)
                let movies_lib_cmd = format!(
                    "curl -s -X POST 'http://localhost:8096/Library/VirtualFolders?name=Films&collectionType=movies&refreshLibrary=true' -H 'X-Emby-Token: {}' -H 'Content-Type: application/json' -d '{{\"LibraryOptions\":{{\"PathInfos\":[{{\"Path\":\"/mnt/decypharr/movies\"}}]}}}}'",
                    jellyfin_token
                );
                ssh::execute_command(host, username, private_key, &movies_lib_cmd).await.ok();
                println!("[Config] Jellyfin: Movies library created");

                // Créer bibliothèque Séries avec LibraryOptions.PathInfos
                let tv_lib_cmd = format!(
                    "curl -s -X POST 'http://localhost:8096/Library/VirtualFolders?name=S%C3%A9ries&collectionType=tvshows&refreshLibrary=true' -H 'X-Emby-Token: {}' -H 'Content-Type: application/json' -d '{{\"LibraryOptions\":{{\"PathInfos\":[{{\"Path\":\"/mnt/decypharr/tv\"}}]}}}}'",
                    jellyfin_token
                );
                ssh::execute_command(host, username, private_key, &tv_lib_cmd).await.ok();
                println!("[Config] Jellyfin: TV library created");
            }
        }
    }

    // 8.3: Configurer Decypharr avec AllDebrid
    emit_progress(&window, "config", 89, "Configuration Decypharr...", None);
    if !config.alldebrid_api_key.is_empty() {
        let ad_key = config.alldebrid_api_key.replace("\\", "\\\\").replace("\"", "\\\"");

        let decypharr_config = format!(r#"{{
  "port": "8282",
  "qbit": {{
    "port": 8282,
    "username": "",
    "password": "",
    "download_folder": "/mnt/decypharr/qbit/downloads",
    "categories": {{
      "radarr": "/mnt/decypharr/movies",
      "sonarr": "/mnt/decypharr/tv"
    }}
  }},
  "debrids": [
    {{
      "name": "alldebrid",
      "enabled": true,
      "api_key": "{}",
      "folder": "/mnt/decypharr/alldebrid",
      "download_uncached": true
    }}
  ],
  "repair": {{
    "enabled": true,
    "interval": "1h"
  }}
}}"#, ad_key);

        let write_config_cmd = format!(
            "cat > ~/media-stack/decypharr/config.json << 'EOFDECYPHARR'\n{}\nEOFDECYPHARR",
            decypharr_config
        );
        ssh::execute_command(host, username, private_key, &write_config_cmd).await.ok();
        // Redémarrer Decypharr en background (évite les timeouts)
        ssh::execute_command(host, username, private_key, "nohup docker restart decypharr > /dev/null 2>&1 &").await.ok();
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        println!("[Config] Decypharr configured with AllDebrid");
    }

    // 8.4: Configurer Radarr/Sonarr
    emit_progress(&window, "config", 91, "Configuration Radarr/Sonarr...", None);
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    let radarr_api = ssh::execute_command(host, username, private_key,
        "grep -oP '(?<=<ApiKey>)[^<]+' ~/media-stack/radarr/config.xml 2>/dev/null || echo ''"
    ).await.unwrap_or_default().trim().to_string();

    let sonarr_api = ssh::execute_command(host, username, private_key,
        "grep -oP '(?<=<ApiKey>)[^<]+' ~/media-stack/sonarr/config.xml 2>/dev/null || echo ''"
    ).await.unwrap_or_default().trim().to_string();

    let prowlarr_api = ssh::execute_command(host, username, private_key,
        "grep -oP '(?<=<ApiKey>)[^<]+' ~/media-stack/prowlarr/config.xml 2>/dev/null || echo ''"
    ).await.unwrap_or_default().trim().to_string();

    // =============================================================================
    // MASTER CONFIG - Fetch dynamique depuis Supabase
    // =============================================================================
    emit_progress(&window, "config", 89, "Récupération de la configuration master...", None);
    println!("[MasterConfig] 🔄 Fetching configuration from Supabase...");

    // Fetch master_config (type "streaming" par défaut, "storage" pour config NAS future)
    let master_config_opt = crate::master_config::fetch_master_config(Some("streaming")).await.ok().flatten();

    if let Some(master_cfg) = &master_config_opt {
        println!("[MasterConfig] ✅ Master config loaded: {}", master_cfg.id);

        // Préparer les variables pour le remplacement de templates
        let mut template_vars = crate::template_engine::TemplateVars::new();
        template_vars.set("PI_IP", host);
        template_vars.set("PI_HOSTNAME", hostname);
        template_vars.set("RADARR_API_KEY", &radarr_api);
        template_vars.set("SONARR_API_KEY", &sonarr_api);
        template_vars.set("PROWLARR_API_KEY", &prowlarr_api);
        template_vars.set("JELLYFIN_USERNAME", &config.jellyfin_username);
        template_vars.set("JELLYFIN_PASSWORD", &config.jellyfin_password);
        template_vars.set("YGG_PASSKEY", config.admin_email.as_deref().unwrap_or(""));
        template_vars.set("ALLDEBRID_API_KEY", &config.alldebrid_api_key);
        template_vars.set("JELLYFIN_API_KEY", "PLACEHOLDER_WILL_BE_EXTRACTED");
        template_vars.set("JELLYFIN_SERVER_ID", "PLACEHOLDER_WILL_BE_EXTRACTED");

        // Appliquer la config pour chaque service depuis master_config
        emit_progress(&window, "config", 90, "Application des configurations master...", None);

        if let Some(jellyseerr_config) = &master_cfg.jellyseerr_config {
            println!("[MasterConfig] Applying Jellyseerr config...");
            if let Err(e) = crate::services::apply_service_config(
                host, username, private_key,
                "jellyseerr",
                jellyseerr_config,
                &template_vars
            ).await {
                println!("[MasterConfig] ⚠️  Jellyseerr config error: {}", e);
            }
        }

        if let Some(radarr_config) = &master_cfg.radarr_config {
            println!("[MasterConfig] Applying Radarr config...");
            if let Err(e) = crate::services::apply_service_config(
                host, username, private_key,
                "radarr",
                radarr_config,
                &template_vars
            ).await {
                println!("[MasterConfig] ⚠️  Radarr config error: {}", e);
            }
        }

        if let Some(sonarr_config) = &master_cfg.sonarr_config {
            println!("[MasterConfig] Applying Sonarr config...");
            if let Err(e) = crate::services::apply_service_config(
                host, username, private_key,
                "sonarr",
                sonarr_config,
                &template_vars
            ).await {
                println!("[MasterConfig] ⚠️  Sonarr config error: {}", e);
            }
        }

        if let Some(prowlarr_config) = &master_cfg.prowlarr_config {
            println!("[MasterConfig] Applying Prowlarr config...");
            if let Err(e) = crate::services::apply_service_config(
                host, username, private_key,
                "prowlarr",
                prowlarr_config,
                &template_vars
            ).await {
                println!("[MasterConfig] ⚠️  Prowlarr config error: {}", e);
            }
        }

        if let Some(jellyfin_config) = &master_cfg.jellyfin_config {
            println!("[MasterConfig] Applying Jellyfin config...");
            if let Err(e) = crate::services::apply_service_config(
                host, username, private_key,
                "jellyfin",
                jellyfin_config,
                &template_vars
            ).await {
                println!("[MasterConfig] ⚠️  Jellyfin config error: {}", e);
            }
        }

        println!("[MasterConfig] ✅ All service configurations applied from master_config");
    } else {
        println!("[MasterConfig] ⚠️  No master_config found - using default configuration");
    }
    // =============================================================================

    // Ajouter Decypharr à Radarr
    if !radarr_api.is_empty() {
        let radarr_client_cmd = format!(r#"curl -s -X POST 'http://localhost:7878/api/v3/downloadclient' \
            -H 'X-Api-Key: {}' \
            -H 'Content-Type: application/json' \
            -d '{{"name": "Decypharr", "implementation": "QBittorrent", "configContract": "QBittorrentSettings", "enable": true, "priority": 1, "fields": [{{"name": "host", "value": "decypharr"}}, {{"name": "port", "value": 8282}}, {{"name": "useSsl", "value": false}}, {{"name": "movieCategory", "value": "radarr"}}]}}'"#, radarr_api);
        ssh::execute_command(host, username, private_key, &radarr_client_cmd).await.ok();
    }

    // Ajouter Decypharr à Sonarr
    if !sonarr_api.is_empty() {
        let sonarr_client_cmd = format!(r#"curl -s -X POST 'http://localhost:8989/api/v3/downloadclient' \
            -H 'X-Api-Key: {}' \
            -H 'Content-Type: application/json' \
            -d '{{"name": "Decypharr", "implementation": "QBittorrent", "configContract": "QBittorrentSettings", "enable": true, "priority": 1, "fields": [{{"name": "host", "value": "decypharr"}}, {{"name": "port", "value": 8282}}, {{"name": "useSsl", "value": false}}, {{"name": "tvCategory", "value": "sonarr"}}]}}'"#, sonarr_api);
        ssh::execute_command(host, username, private_key, &sonarr_client_cmd).await.ok();
    }

    // 8.4b: Ajouter les Root Folders
    if !radarr_api.is_empty() {
        let radarr_root_cmd = format!(r#"curl -s -X POST 'http://localhost:7878/api/v3/rootfolder' \
            -H 'X-Api-Key: {}' -H 'Content-Type: application/json' \
            -d '{{"path": "/mnt/decypharr/movies"}}'"#, radarr_api);
        ssh::execute_command(host, username, private_key, &radarr_root_cmd).await.ok();
    }

    if !sonarr_api.is_empty() {
        let sonarr_root_cmd = format!(r#"curl -s -X POST 'http://localhost:8989/api/v3/rootfolder' \
            -H 'X-Api-Key: {}' -H 'Content-Type: application/json' \
            -d '{{"path": "/mnt/decypharr/tv"}}'"#, sonarr_api);
        ssh::execute_command(host, username, private_key, &sonarr_root_cmd).await.ok();
    }

    // 8.5: Configurer Prowlarr avec YGG
    emit_progress(&window, "config", 94, "Configuration Prowlarr...", None);
    if let Some(ref ygg_passkey) = config.ygg_passkey {
        if !ygg_passkey.is_empty() && !prowlarr_api.is_empty() {
            let passkey = ygg_passkey.replace("\\", "\\\\").replace("\"", "\\\"");

            let prowlarr_ygg_cmd = format!(r#"curl -s -X POST 'http://localhost:9696/api/v1/indexer' \
                -H 'X-Api-Key: {}' \
                -H 'Content-Type: application/json' \
                -d '{{"name": "YGGTorrent", "definitionName": "yggtorrent", "implementation": "YggTorrent", "configContract": "YggTorrentSettings", "enable": true, "protocol": "torrent", "priority": 1, "fields": [{{"name": "passkey", "value": "{}"}}]}}'"#, prowlarr_api, passkey);
            ssh::execute_command(host, username, private_key, &prowlarr_ygg_cmd).await.ok();

            // Ajouter FlareSolverr
            let flaresolverr_cmd = format!(r#"curl -s -X POST 'http://localhost:9696/api/v1/indexerProxy' \
                -H 'X-Api-Key: {}' \
                -H 'Content-Type: application/json' \
                -d '{{"name": "FlareSolverr", "configContract": "FlareSolverrSettings", "implementation": "FlareSolverr", "fields": [{{"name": "host", "value": "http://localhost:8191"}}]}}'"#, prowlarr_api);
            ssh::execute_command(host, username, private_key, &flaresolverr_cmd).await.ok();
        }
    }

    // 8.6: Synchroniser Prowlarr avec Radarr/Sonarr
    if !prowlarr_api.is_empty() {
        emit_progress(&window, "config", 96, "Synchronisation Prowlarr...", None);

        if !radarr_api.is_empty() {
            let sync_radarr_cmd = format!(r#"curl -s -X POST 'http://localhost:9696/api/v1/applications' \
                -H 'X-Api-Key: {}' \
                -H 'Content-Type: application/json' \
                -d '{{"name": "Radarr", "syncLevel": "fullSync", "implementation": "Radarr", "configContract": "RadarrSettings", "fields": [{{"name": "prowlarrUrl", "value": "http://localhost:9696"}}, {{"name": "baseUrl", "value": "http://localhost:7878"}}, {{"name": "apiKey", "value": "{}"}}]}}'"#, prowlarr_api, radarr_api);
            ssh::execute_command(host, username, private_key, &sync_radarr_cmd).await.ok();
        }

        if !sonarr_api.is_empty() {
            let sync_sonarr_cmd = format!(r#"curl -s -X POST 'http://localhost:9696/api/v1/applications' \
                -H 'X-Api-Key: {}' \
                -H 'Content-Type: application/json' \
                -d '{{"name": "Sonarr", "syncLevel": "fullSync", "implementation": "Sonarr", "configContract": "SonarrSettings", "fields": [{{"name": "prowlarrUrl", "value": "http://localhost:9696"}}, {{"name": "baseUrl", "value": "http://localhost:8989"}}, {{"name": "apiKey", "value": "{}"}}]}}'"#, prowlarr_api, sonarr_api);
            ssh::execute_command(host, username, private_key, &sync_sonarr_cmd).await.ok();
        }
    }

    // 8.7: Configurer Bazarr
    emit_progress(&window, "config", 97, "Configuration Bazarr...", None);
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    let mut bazarr_ready = false;
    for _ in 0..12 {
        let check = ssh::execute_command(host, username, private_key,
            "test -f ~/media-stack/bazarr/config/config.yaml && echo OK || echo WAIT"
        ).await.unwrap_or_default();
        if check.contains("OK") {
            bazarr_ready = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }

    if bazarr_ready && !radarr_api.is_empty() && !sonarr_api.is_empty() {
        let bazarr_api_check = ssh::execute_command(host, username, private_key,
            "grep -oP '(?<=apikey: )[^\\s]+' ~/media-stack/bazarr/config/config.yaml 2>/dev/null || echo ''"
        ).await.unwrap_or_default().trim().to_string();

        if !bazarr_api_check.is_empty() {
            let bazarr_radarr_cmd = format!(r#"curl -s -X POST 'http://localhost:6767/api/system/settings' \
                -H 'X-API-KEY: {}' -H 'Content-Type: application/json' \
                -d '{{"settings": {{"radarr": {{"ip": "localhost", "port": 7878, "apikey": "{}", "ssl": false, "base_url": ""}}}}}}"#,
                bazarr_api_check, radarr_api);
            ssh::execute_command(host, username, private_key, &bazarr_radarr_cmd).await.ok();

            let bazarr_sonarr_cmd = format!(r#"curl -s -X POST 'http://localhost:6767/api/system/settings' \
                -H 'X-API-KEY: {}' -H 'Content-Type: application/json' \
                -d '{{"settings": {{"sonarr": {{"ip": "localhost", "port": 8989, "apikey": "{}", "ssl": false, "base_url": ""}}}}}}"#,
                bazarr_api_check, sonarr_api);
            ssh::execute_command(host, username, private_key, &bazarr_sonarr_cmd).await.ok();
            println!("[Config] Bazarr: Configured");
        }
    }

    // 8.8: Configuration automatique de Jellyseerr via API
    emit_progress(&window, "config", 96, "Configuration de Jellyseerr...", None);
    println!("[Config] Jellyseerr: Starting automatic configuration...");

    // Attendre que Jellyseerr soit prêt (max 60 sec)
    let mut jellyseerr_ready = false;
    for i in 0..12 {
        let check = ssh::execute_command(host, username, private_key,
            "curl -s -o /dev/null -w '%{http_code}' 'http://localhost:5055/api/v1/status' 2>/dev/null || echo '000'"
        ).await.unwrap_or_default();

        if check.trim() == "200" || check.trim() == "403" {
            jellyseerr_ready = true;
            println!("[Config] Jellyseerr: Service ready after {} seconds", (i + 1) * 5);
            break;
        }
        println!("[Config] Jellyseerr: Waiting... (attempt {}/12)", i + 1);
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }

    if jellyseerr_ready {
        // Petite pause pour s'assurer que Jellyseerr est complètement prêt
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;

        // Étape 1: Authentifier avec Jellyfin et créer l'admin
        // IMPORTANT: Échapper les caractères spéciaux pour JSON
        let jf_user = config.jellyfin_username.replace("\\", "\\\\").replace("\"", "\\\"");
        let jf_pass = config.jellyfin_password.replace("\\", "\\\\").replace("\"", "\\\"");

        // Essayer plusieurs hostnames jusqu'à ce qu'un fonctionne
        // 1. host.docker.internal (avec extra_hosts configuré)
        // 2. jellyfin (nom du service Docker sur le même réseau)
        // 3. IP du Pi (passée en paramètre host)
        let hostnames_to_try = vec![
            "host.docker.internal".to_string(),
            "jellyfin".to_string(),
            host.to_string(),
        ];

        let mut auth_result = String::new();
        for jellyfin_hostname in &hostnames_to_try {
            println!("[Config] Jellyseerr: Trying hostname: {}", jellyfin_hostname);
            // serverType: 2 = JELLYFIN (enum MediaServerType)
            // urlBase: "" évite que JavaScript ajoute "undefined" à l'URL
            let auth_cmd = format!(
                r#"curl -s -X POST 'http://localhost:5055/api/v1/auth/jellyfin' \
                   -H 'Content-Type: application/json' \
                   -c /tmp/jellyseerr_cookies.txt \
                   -d '{{"username":"{}","password":"{}","hostname":"{}","port":8096,"useSsl":false,"urlBase":"","serverType":2,"email":"admin@easyjelly.local"}}'"#,
                jf_user, jf_pass, jellyfin_hostname
            );
            auth_result = ssh::execute_command(host, username, private_key, &auth_cmd).await.unwrap_or_default();
            println!("[Config] Jellyseerr: Auth result with {}: {}", jellyfin_hostname, &auth_result[..std::cmp::min(200, auth_result.len())]);

            if auth_result.contains("\"id\"") {
                println!("[Config] Jellyseerr: Success with hostname: {}", jellyfin_hostname);
                break;
            }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }

        // Vérifier si l'auth a réussi (contient "id" dans la réponse)
        if auth_result.contains("\"id\"") {
            println!("[Config] Jellyseerr: Admin user created successfully!");

            // Étape 2: Sync des bibliothèques Jellyfin
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            let sync_cmd = "curl -s -X GET 'http://localhost:5055/api/v1/settings/jellyfin/library?sync=true' -b /tmp/jellyseerr_cookies.txt";
            let sync_result = ssh::execute_command(host, username, private_key, sync_cmd).await.unwrap_or_default();
            println!("[Config] Jellyseerr: Library sync result: {}", &sync_result[..std::cmp::min(300, sync_result.len())]);

            // Extraire les IDs des bibliothèques
            let mut library_ids: Vec<String> = Vec::new();
            let mut search_pos = 0;
            while let Some(id_start) = sync_result[search_pos..].find("\"id\":\"") {
                let actual_start = search_pos + id_start + 6;
                if let Some(id_end) = sync_result[actual_start..].find("\"") {
                    let lib_id = &sync_result[actual_start..actual_start + id_end];
                    library_ids.push(lib_id.to_string());
                    search_pos = actual_start + id_end;
                } else {
                    break;
                }
            }

            // Étape 3: Activer toutes les bibliothèques trouvées
            if !library_ids.is_empty() {
                let ids_str = library_ids.join(",");
                let enable_cmd = format!(
                    "curl -s -X GET 'http://localhost:5055/api/v1/settings/jellyfin/library?enable={}' -b /tmp/jellyseerr_cookies.txt",
                    ids_str
                );
                ssh::execute_command(host, username, private_key, &enable_cmd).await.ok();
                println!("[Config] Jellyseerr: Enabled {} libraries: {}", library_ids.len(), ids_str);
            }

            // Étape 4: Finaliser le setup
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            let init_cmd = "curl -s -X POST 'http://localhost:5055/api/v1/settings/initialize' -b /tmp/jellyseerr_cookies.txt -H 'Content-Type: application/json'";
            let init_result = ssh::execute_command(host, username, private_key, init_cmd).await.unwrap_or_default();
            println!("[Config] Jellyseerr: Initialize result: {}", init_result);

            // Nettoyer les cookies
            ssh::execute_command(host, username, private_key, "rm -f /tmp/jellyseerr_cookies.txt").await.ok();

            println!("[Config] Jellyseerr: Configuration completed successfully!");
        } else {
            println!("[Config] Jellyseerr: Auth failed, manual setup required at http://<pi-ip>:5055");
        }
    } else {
        println!("[Config] Jellyseerr: Service not ready after 60 seconds, manual setup required");
    }

    ssh::execute_command(host, username, private_key,
        "echo \"$(date): Service configuration completed\" >> ~/jellysetup-logs/install.log"
    ).await.ok();

    // 8.9: Sauvegarder l'installation dans Supabase (centralisation des identifiants)
    emit_progress(&window, "supabase", 98, "Sauvegarde dans le cloud...", None);

    // Récupérer le fingerprint SSH (capturé lors de la connexion)
    let ssh_fingerprint = ssh::get_last_host_fingerprint();

    // Sauvegarder dans Supabase (ne bloque pas en cas d'erreur)
    // Note: Pour l'auth par clé, on pourrait aussi sauvegarder les clés SSH
    // mais elles ne sont pas passées à cette fonction actuellement
    match crate::supabase::save_installation(
        hostname,
        host,
        None,  // TODO: Ajouter la clé publique à InstallConfig
        None,  // TODO: Ajouter la clé privée chiffrée
        ssh_fingerprint.as_deref(),
        env!("CARGO_PKG_VERSION"),
    ).await {
        Ok(config_id) => {
            println!("[Supabase] Installation saved with ID: {}", config_id);

            // Sauvegarder aussi les credentials de l'utilisateur
            if let Err(e) = crate::supabase::save_pi_config(
                hostname,
                &config_id,
                Some(&config.alldebrid_api_key),
                config.ygg_passkey.as_deref(),
                config.cloudflare_token.as_deref(),
                None, // jellyfin_api_key
                None, // radarr_api_key
                None, // sonarr_api_key
                None, // prowlarr_api_key
            ).await {
                println!("[Supabase] Warning: could not save Pi config: {}", e);
            }

            // Mettre à jour le statut à "completed"
            if let Err(e) = crate::supabase::update_status(hostname, &config_id, "completed", None).await {
                println!("[Supabase] Warning: could not update status: {}", e);
            }
        }
        Err(e) => {
            println!("[Supabase] Warning: could not save installation: {}", e);
        }
    }

    emit_progress(&window, "complete", 100, "Installation terminée !", None);

    tracing::info!("Installation completed successfully on {}", host);
    Ok(())
}

/// Émet un événement de progression vers le frontend
fn emit_progress(window: &Window, step: &str, percent: u32, message: &str, speed: Option<&str>) {
    emit_progress_with_auth(window, step, percent, message, speed, None);
}

/// Émet un événement de progression avec données d'authentification Jellyfin optionnelles
fn emit_progress_with_auth(window: &Window, step: &str, percent: u32, message: &str, speed: Option<&str>, jellyfin_auth: Option<JellyfinAuth>) {
    let _ = window.emit(
        "flash-progress",
        FlashProgress {
            step: step.to_string(),
            percent,
            message: message.to_string(),
            speed: speed.map(String::from),
            jellyfin_auth,
        },
    );
}

/// Exécute l'installation complète sur le Pi via SSH (authentification par mot de passe)
pub async fn run_full_installation_password(
    window: Window,
    host: &str,
    username: &str,
    password: &str,
    config: InstallConfig,
) -> Result<()> {
    use crate::ssh;

    // Empêcher la mise en veille du Mac pendant l'installation
    #[cfg(target_os = "macos")]
    let caffeinate_process = {
        match std::process::Command::new("caffeinate")
            .args(["-dims"]) // display, idle, disk, system sleep prevention
            .spawn()
        {
            Ok(child) => {
                println!("[Install] caffeinate started (PID: {})", child.id());
                Some(child)
            }
            Err(e) => {
                println!("[Install] Warning: could not start caffeinate: {}", e);
                None
            }
        }
    };
    #[cfg(not(target_os = "macos"))]
    let caffeinate_process: Option<std::process::Child> = None;

    // IMPORTANT: Nettoyer le known_hosts local pour cette IP
    // Cela permet de gérer les reflash de carte SD sans erreur de clé SSH
    if let Err(e) = ssh::clear_known_hosts_for_ip(host) {
        println!("[Install] Warning: could not clear known_hosts: {}", e);
    }

    // Faire une première connexion SSH pour capturer le fingerprint du serveur
    emit_progress(&window, "ssh_check", 0, "Vérification de la connexion SSH...", None);
    match ssh::test_connection_password(host, username, password).await {
        Ok(true) => {
            // Récupérer le fingerprint capturé
            if let Some(fp) = ssh::get_last_host_fingerprint() {
                println!("[Install] SSH host fingerprint captured: {}", fp);
                // Le fingerprint sera sauvegardé dans Supabase avec les autres données
            }
        }
        Ok(false) => {
            return Err(anyhow::anyhow!("Authentification SSH échouée"));
        }
        Err(e) => {
            return Err(anyhow::anyhow!("Connexion SSH impossible: {}", e));
        }
    }

    // Initialiser la session persistante
    if let Err(e) = ssh::init_persistent_session(host, username, password).await {
        println!("[Install] Warning: could not init persistent SSH session: {}", e);
    } else {
        println!("[Install] ✅ Persistent SSH session initialized");
    }

    // Notifier le frontend que la connexion SSH est OK
    emit_progress(&window, "ssh_connected", 5, "Connexion SSH établie", None);

    // Récupérer le vrai hostname du Pi via SSH (important pour les connexions par IP)
    let hostname = if host.contains(".local") {
        // Si c'est déjà un hostname mDNS, on retire juste .local
        host.replace(".local", "")
    } else {
        // Sinon on récupère le hostname via SSH (one-shot, ça marche)
        match ssh::execute_command_password(host, username, password, "hostname").await {
            Ok(h) => {
                let h = h.trim().to_string();
                println!("[Install] Hostname récupéré via SSH: {}", h);
                h
            }
            Err(e) => {
                println!("[Install] Warning: impossible de récupérer hostname: {}, utilisation de l'IP", e);
                host.to_string()
            }
        }
    };

    // Générer le docker-compose.yml avec tous les services
    let docker_compose = generate_docker_compose(
        &hostname,
        config.cloudflare_token.as_deref()
    );

    // ==========================================================================
    // MEGA SYSTÈME DE LOGS - Initialisation
    // ==========================================================================
    use crate::logging::{InstallationLogger, LogLevel};

    let logger = InstallationLogger::new(
        &hostname,           // pi_name (utilisé pour le schéma Supabase)
        host,                // pi_ip
        host,                // ssh_host
        username,            // ssh_username
        password,            // ssh_password
        env!("CARGO_PKG_VERSION"), // installer_version
    );

    // Initialiser le logger (crée dossier local + schéma Supabase)
    if let Err(e) = logger.initialize().await {
        println!("[Install] ⚠️ Warning: logger init failed: {}", e);
    }

    logger.log_with_details(
        LogLevel::Info,
        "installation_start",
        "Installation démarrée",
        serde_json::json!({
            "hostname": hostname,
            "host": host,
            "username": username,
            "alldebrid_configured": !config.alldebrid_api_key.is_empty(),
            "cloudflare_configured": config.cloudflare_token.is_some(),
            "ygg_configured": config.ygg_passkey.is_some(),
        })
    ).await;

    // Étape 1: Mise à jour système (en background pour éviter timeout)
    logger.start_step("apt_update").await;
    emit_progress(&window, "update", 0, "Mise à jour système (peut prendre 10-15 min)...", None);

    // Lancer apt update/upgrade en background avec nohup
    // IMPORTANT: DEBIAN_FRONTEND=noninteractive + --force-confdef/confold pour éviter les questions interactives
    let update_cmd = format!(
        "nohup sh -c 'export DEBIAN_FRONTEND=noninteractive && echo \"{}\" | sudo -S -E apt update && echo \"{}\" | sudo -S -E apt upgrade -y -o Dpkg::Options::=\"--force-confdef\" -o Dpkg::Options::=\"--force-confold\" && echo \"{}\" | sudo -S -E apt install -y git curl && touch /tmp/apt_done' > /tmp/apt.log 2>&1 &",
        password, password, password
    );
    ssh::execute_command_password(host, username, password, &update_cmd).await.ok();

    // Attendre que apt soit terminé (max 15 min)
    let mut apt_completed = false;
    for i in 0..90 {
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;

        // Vérifier si apt est terminé et récupérer le paquet en cours
        let status_cmd = r#"
            if [ -f /tmp/apt_done ]; then
                echo 'DONE'
            elif pgrep -f 'apt|dpkg' > /dev/null; then
                # Déterminer la phase et récupérer l'info pertinente
                if grep -q 'Unpacking\|Setting up' /tmp/apt.log 2>/dev/null; then
                    # Phase upgrade: afficher le paquet
                    PKG=$(grep -oE '(Unpacking|Setting up) [^ ]+' /tmp/apt.log 2>/dev/null | tail -1 | awk '{print $2}' | cut -d: -f1)
                    echo "UPGRADE:${PKG:-installing}"
                elif grep -q 'Reading package lists\|Building dependency' /tmp/apt.log 2>/dev/null; then
                    echo "UPDATE:refresh"
                elif grep -q 'Hit:\|Get:' /tmp/apt.log 2>/dev/null; then
                    REPO=$(grep -oE '(Hit|Get):[0-9]+ [^ ]+' /tmp/apt.log 2>/dev/null | tail -1 | awk '{print $2}' | cut -d'/' -f3)
                    echo "FETCH:${REPO:-repos}"
                else
                    echo "RUNNING:working"
                fi
            else
                echo 'IDLE'
            fi
        "#;
        match ssh::execute_command_password(host, username, password, status_cmd).await {
            Ok(output) => {
                let output = output.trim();
                if output.contains("DONE") {
                    println!("[Install] apt upgrade completed!");
                    apt_completed = true;
                    break;
                } else if output.starts_with("UPGRADE:") {
                    // Phase upgrade: afficher le paquet
                    let pkg = output.strip_prefix("UPGRADE:").unwrap_or("...");
                    let progress_msg = format!("Installation: {} • ~{}min", pkg, (15 - i / 6).max(1));
                    emit_progress(&window, "update", (i as u32).min(14), &progress_msg, None);
                } else if output.starts_with("UPDATE:") {
                    let progress_msg = format!("Analyse des paquets... • ~{}min", (15 - i / 6).max(1));
                    emit_progress(&window, "update", (i as u32).min(14), &progress_msg, None);
                } else if output.starts_with("FETCH:") {
                    let repo = output.strip_prefix("FETCH:").unwrap_or("repos");
                    let progress_msg = format!("Téléchargement: {} • ~{}min", repo, (15 - i / 6).max(1));
                    emit_progress(&window, "update", (i as u32).min(14), &progress_msg, None);
                } else if output.starts_with("RUNNING:") {
                    let progress_msg = format!("Mise à jour en cours... • ~{}min", (15 - i / 6).max(1));
                    emit_progress(&window, "update", (i as u32).min(14), &progress_msg, None);
                } else {
                    // IDLE = apt pas en cours, mais pas forcément terminé (peut avoir rebooté)
                    println!("[Install] apt not running, checking if completed...");
                    // Ne pas break ici, continuer à vérifier
                }
            }
            Err(_) => {
                // Pi probablement en train de rebooter (kernel update)
                println!("[Install] SSH lost, waiting for Pi...");
                emit_progress(&window, "update", 10, "Pi redémarre (kernel update)...", None);
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;

                // Attendre que le Pi revienne
                for _j in 0..30 {
                    if ssh::execute_command_password(host, username, password, "echo ok").await.is_ok() {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                }
                // Après reboot, continuer la boucle pour vérifier apt_done
            }
        }

        if i == 89 {
            println!("[Install] Warning: apt timeout, continuing anyway");
        }
    }

    // Si apt n'a pas terminé proprement (ex: reboot pendant upgrade), réparer et relancer
    if !apt_completed {
        println!("[Install] apt may have been interrupted, checking for broken packages...");
        emit_progress(&window, "update", 12, "Vérification des paquets...", None);

        // Réparer les paquets potentiellement cassés
        let repair_cmd = format!(
            "echo '{}' | sudo -S DEBIAN_FRONTEND=noninteractive dpkg --configure -a && echo '{}' | sudo -S DEBIAN_FRONTEND=noninteractive apt --fix-broken install -y -o Dpkg::Options::='--force-confdef' -o Dpkg::Options::='--force-confold'",
            password, password
        );
        ssh::execute_command_password(host, username, password, &repair_cmd).await.ok();

        // Vérifier si on doit relancer l'upgrade
        let check_upgrade = ssh::execute_command_password(host, username, password,
            "apt list --upgradable 2>/dev/null | grep -c upgradable || echo 0"
        ).await.unwrap_or_default();

        let upgradable_count: i32 = check_upgrade.trim().parse().unwrap_or(0);
        if upgradable_count > 5 {
            println!("[Install] {} packages still need upgrading, resuming...", upgradable_count);
            emit_progress(&window, "update", 13, &format!("Reprise upgrade ({} paquets)...", upgradable_count), None);

            let resume_cmd = format!(
                "echo '{}' | sudo -S DEBIAN_FRONTEND=noninteractive apt upgrade -y -o Dpkg::Options::='--force-confdef' -o Dpkg::Options::='--force-confold'",
                password
            );
            ssh::execute_command_password(host, username, password, &resume_cmd).await.ok();
        }
    }

    // Logger la fin de l'étape apt
    logger.end_step("apt_update", apt_completed).await;
    logger.log(LogLevel::Info, "apt_update", &format!(
        "APT terminé: {}",
        if apt_completed { "succès" } else { "avec récupération" }
    )).await;

    // IMPORTANT: Attendre que APT soit complètement libre avant Docker
    // (évite "Could not get lock /var/lib/dpkg/lock-frontend")
    emit_progress(&window, "docker", 14, "Attente fin des mises à jour...", None);
    for wait_i in 0..60 {  // Max 5 minutes
        let apt_free = ssh::execute_command_password(host, username, password,
            "timeout 5 fuser /var/lib/dpkg/lock /var/lib/dpkg/lock-frontend /var/lib/apt/lists/lock /var/cache/apt/archives/lock 2>/dev/null; RC=$?; if [ $RC -eq 1 ] || [ $RC -eq 124 ]; then echo FREE; else echo LOCKED; fi"
        ).await.unwrap_or_default();

        if apt_free.contains("FREE") {
            println!("[Install] APT is free, proceeding with Docker install");
            break;
        }
        println!("[Install] APT still locked (attempt {}/60), waiting 5s...", wait_i + 1);
        if wait_i % 6 == 0 {
            emit_progress(&window, "docker", 14, &format!("APT verrouillé, attente... (~{}s)", (60 - wait_i) * 5), None);
        }
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }

    // Étape 2: Installation Docker
    logger.start_step("docker_install").await;
    emit_progress(&window, "docker", 15, "Vérification Docker...", None);

    // Vérifier si Docker est déjà installé
    let docker_check = ssh::execute_command_password(host, username, password, "docker --version 2>&1").await;
    logger.log(LogLevel::Debug, "docker_install", &format!("Docker check: {:?}", docker_check)).await;

    let docker_output = docker_check.as_ref().map(|s| s.as_str()).unwrap_or("");
    let docker_installed = docker_check.is_ok() && docker_output.contains("Docker");
    println!("[Install] Docker installed: {}, output: '{}'", docker_installed, docker_output.trim());

    let mut needs_reboot = false;

    if !docker_installed {
        // Logger l'action
        ssh::execute_command_password(host, username, password,
            "echo \"$(date): Installing Docker...\" >> ~/jellysetup-logs/install.log"
        ).await.ok();

        let docker_cmd = format!(
            "curl -fsSL https://get.docker.com -o /tmp/get-docker.sh && echo '{}' | sudo -S sh /tmp/get-docker.sh 2>&1 | tee -a ~/jellysetup-logs/docker-install.log && echo '{}' | sudo -S usermod -aG docker $USER",
            password, password
        );
        match ssh::execute_command_password(host, username, password, &docker_cmd).await {
            Ok(output) => {
                println!("[Install] Docker install output: {}", &output[..output.len().min(500)]);
                ssh::execute_command_password(host, username, password,
                    "echo \"$(date): Docker install completed\" >> ~/jellysetup-logs/install.log"
                ).await.ok();
            }
            Err(e) => {
                let error_msg = format!("Docker install failed: {}", e);
                println!("[Install] ERROR: {}", error_msg);
                emit_progress(&window, "docker", 15, &format!("❌ Erreur: {}", e), None);
                ssh::execute_command_password(host, username, password,
                    &format!("echo \"$(date): ERROR - {}\" >> ~/jellysetup-logs/install.log", error_msg)
                ).await.ok();
                return Err(anyhow!(error_msg));
            }
        }
        // Docker vient d'être installé, on doit rebooter pour le groupe docker
        needs_reboot = true;
    } else {
        println!("[Install] Docker already installed, skipping");
        ssh::execute_command_password(host, username, password,
            "echo \"$(date): Docker already installed\" >> ~/jellysetup-logs/install.log"
        ).await.ok();

        // Vérifier si l'utilisateur peut utiliser docker sans sudo (groupe docker appliqué)
        let docker_test = ssh::execute_command_password(host, username, password,
            "docker ps 2>&1"
        ).await;

        if let Ok(output) = &docker_test {
            if output.contains("permission denied") || output.contains("Cannot connect") {
                println!("[Install] User not in docker group yet, reboot needed");
                needs_reboot = true;
            } else {
                println!("[Install] Docker works without sudo, no reboot needed");
                needs_reboot = false;
            }
        } else {
            println!("[Install] Docker test failed, reboot to be safe");
            needs_reboot = true;
        }
    }

    // Étape 3: Redémarrage pour appliquer groupe docker (seulement si nécessaire)
    if needs_reboot {
        println!("[Install] ========== REBOOT ==========");
        emit_progress(&window, "reboot", 30, "Redémarrage...", None);
        ssh::execute_command_password(host, username, password,
            "echo \"$(date): Rebooting to apply docker group...\" >> ~/jellysetup-logs/install.log"
        ).await.ok();
        let reboot_cmd = format!("echo '{}' | sudo -S reboot", password);
        ssh::execute_command_password(host, username, password, &reboot_cmd).await.ok();
        println!("[Install] Reboot command sent, waiting 45s...");
        tokio::time::sleep(std::time::Duration::from_secs(45)).await;

        // Attendre que le Pi soit de nouveau accessible
        println!("[Install] Waiting for Pi to come back online...");
        let mut pi_back = false;
        for i in 0..30 {
            match ssh::execute_command_password(host, username, password, "echo ok").await {
                Ok(_) => {
                    println!("[Install] Pi is back online after {} attempts", i + 1);
                    pi_back = true;
                    break;
                }
                Err(e) => {
                    println!("[Install] Pi not yet responding (attempt {}/30): {}", i + 1, e);
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
        if !pi_back {
            return Err(anyhow!("Pi not responding after reboot (30 attempts)"));
        }
    } else {
        println!("[Install] Skipping reboot - Docker already working");
        emit_progress(&window, "reboot", 30, "Reboot non nécessaire", None);
    }

    // Vérifier que Docker est bien installé après le reboot
    println!("[Install] Checking Docker after reboot...");
    let docker_verify = ssh::execute_command_password(host, username, password, "docker --version 2>&1").await;
    println!("[Install] Docker verify result: {:?}", docker_verify);

    let docker_verify_output = docker_verify.as_ref().map(|s| s.as_str()).unwrap_or("");
    let docker_ok_after_reboot = docker_verify.is_ok() && docker_verify_output.contains("Docker");
    println!("[Install] Docker OK after reboot: {}", docker_ok_after_reboot);

    if !docker_ok_after_reboot {
        // Docker pas installé, réessayer
        println!("[Install] Docker not found after reboot, attempting 2nd installation...");
        emit_progress(&window, "docker", 20, "Installation Docker (2ème tentative)...", None);
        ssh::execute_command_password(host, username, password,
            "echo \"$(date): Docker not found after reboot, retrying...\" >> ~/jellysetup-logs/install.log"
        ).await.ok();

        // IMPORTANT: Attendre que APT soit libre avant 2ème tentative
        for wait_i in 0..60 {
            let apt_free = ssh::execute_command_password(host, username, password,
                "timeout 5 fuser /var/lib/dpkg/lock /var/lib/dpkg/lock-frontend /var/lib/apt/lists/lock /var/cache/apt/archives/lock 2>/dev/null; RC=$?; if [ $RC -eq 1 ] || [ $RC -eq 124 ]; then echo FREE; else echo LOCKED; fi"
            ).await.unwrap_or_default();
            if apt_free.contains("FREE") {
                println!("[Install] APT is free for Docker retry");
                break;
            }
            println!("[Install] APT still locked before Docker retry (attempt {}/60)", wait_i + 1);
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }

        let docker_cmd = format!(
            "curl -fsSL https://get.docker.com -o /tmp/get-docker.sh && echo '{}' | sudo -S sh /tmp/get-docker.sh 2>&1 | tee -a ~/jellysetup-logs/docker-install-retry.log && echo '{}' | sudo -S usermod -aG docker $USER",
            password, password
        );
        ssh::execute_command_password(host, username, password, &docker_cmd).await?;

        // Nouveau reboot après install Docker
        let reboot_cmd = format!("echo '{}' | sudo -S reboot", password);
        ssh::execute_command_password(host, username, password, &reboot_cmd).await.ok();
        tokio::time::sleep(std::time::Duration::from_secs(45)).await;

        // Attendre le Pi
        for i in 0..30 {
            if ssh::execute_command_password(host, username, password, "echo ok").await.is_ok() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            if i == 29 {
                return Err(anyhow!("Pi not responding after Docker reboot"));
            }
        }
    }

    // VÉRIFICATION FINALE OBLIGATOIRE: Docker DOIT être installé avant de continuer
    println!("[Install] ========== DOCKER FINAL VERIFICATION ==========");
    emit_progress(&window, "docker", 35, "Vérification Docker...", None);
    let final_docker_check = ssh::execute_command_password(host, username, password,
        "docker --version 2>&1 && docker compose version 2>&1"
    ).await;

    println!("[Install] Final Docker check result: {:?}", final_docker_check);

    match &final_docker_check {
        Ok(output) if output.contains("Docker") && output.contains("Docker Compose") => {
            println!("[Install] ✅ Docker et Docker Compose vérifiés: {}", output.lines().next().unwrap_or(""));
            ssh::execute_command_password(host, username, password,
                &format!("echo \"$(date): Docker verified - {}\" >> ~/jellysetup-logs/install.log",
                    output.lines().next().unwrap_or("ok").replace('"', "'"))
            ).await.ok();
        }
        Ok(output) => {
            // Docker check returned but doesn't contain expected strings
            println!("[Install] ❌ Docker check returned unexpected output: '{}'", output);
            let error_msg = format!("❌ FATAL: Docker n'est pas installé correctement. Output: {}", output.chars().take(200).collect::<String>());
            emit_progress(&window, "docker", 35, "❌ Docker non installé", None);
            ssh::execute_command_password(host, username, password,
                &format!("echo \"$(date): FATAL ERROR - Docker check failed: {}\" >> ~/jellysetup-logs/install.log",
                    output.chars().take(100).collect::<String>().replace('"', "'"))
            ).await.ok();
            return Err(anyhow!(error_msg));
        }
        Err(e) => {
            println!("[Install] ❌ Docker check failed with error: {}", e);
            let error_msg = format!("❌ FATAL: Docker n'est pas installé. Erreur SSH: {}", e);
            emit_progress(&window, "docker", 35, "❌ Docker non installé", None);
            ssh::execute_command_password(host, username, password,
                &format!("echo \"$(date): FATAL ERROR - Docker not installed, SSH error\" >> ~/jellysetup-logs/install.log")
            ).await.ok();
            return Err(anyhow!(error_msg));
        }
    }

    println!("[Install] ========== DOCKER OK - CONTINUING ==========");

    // Étape 4: Création de la structure (y compris les dossiers media)
    emit_progress(&window, "structure", 40, "Création structure...", None);
    let mkdir_cmd = format!(
        "mkdir -p ~/media-stack/{{decypharr,jellyfin,radarr,sonarr,prowlarr,jellyseerr,bazarr,logs}} && \
         echo '{}' | sudo -S mkdir -p /mnt/decypharr/{{movies,tv,qbit/downloads}} && \
         echo '{}' | sudo -S chown -R $USER:$USER /mnt/decypharr",
        password, password
    );
    ssh::execute_command_password(host, username, password, &mkdir_cmd).await?;

    // Étape 5: Écrire le docker-compose.yml
    emit_progress(&window, "compose_write", 50, "Génération docker-compose.yml...", None);
    let write_cmd = format!("cat > ~/media-stack/docker-compose.yml << 'EOFCOMPOSE'\n{}\nEOFCOMPOSE", docker_compose);
    ssh::execute_command_password(host, username, password, &write_cmd).await?;

    // Étape 6: Démarrer les services (en background car pull peut être très long)
    emit_progress(&window, "compose_up", 60, "Téléchargement des images Docker (peut prendre 10-20 min)...", None);

    // Vérifier que Docker fonctionne avant de lancer le pull
    let docker_test = ssh::execute_command_password(host, username, password, "docker ps").await;
    if docker_test.is_err() {
        let error_msg = "Docker n'est pas accessible - vérifiez l'installation";
        emit_progress(&window, "compose_up", 60, &format!("❌ {}", error_msg), None);
        ssh::execute_command_password(host, username, password,
            &format!("echo \"$(date): ERROR - {}\" >> ~/jellysetup-logs/install.log", error_msg)
        ).await.ok();
        return Err(anyhow!(error_msg));
    }

    // Docker compose pull avec retry automatique en cas d'échec réseau
    let mut pull_attempt = 0;
    let max_pull_attempts = 3;

    'pull_loop: loop {
        pull_attempt += 1;
        if pull_attempt > max_pull_attempts {
            let error_msg = format!("Docker pull échoué après {} tentatives", max_pull_attempts);
            emit_progress(&window, "compose_up", 60, &format!("❌ {}", error_msg), None);
            return Err(anyhow!(error_msg));
        }

        // Logger et lancer docker compose pull
        ssh::execute_command_password(host, username, password,
            &format!("echo \"$(date): Starting docker compose pull (attempt {}/{})...\" >> ~/jellysetup-logs/install.log", pull_attempt, max_pull_attempts)
        ).await.ok();

        emit_progress(&window, "compose_up", 60, &format!("Téléchargement images (tentative {}/{})...", pull_attempt, max_pull_attempts), None);

        // Lancer docker compose pull avec fichier marker de fin (évite le bug pgrep/nohup)
        ssh::execute_command_password(host, username, password,
            "rm -f /tmp/docker_pull_done /tmp/docker_pull_failed && cd ~/media-stack && (docker compose pull > ~/jellysetup-logs/docker_pull.log 2>&1 && touch /tmp/docker_pull_done || touch /tmp/docker_pull_failed) &"
        ).await?;

        // Attendre que le pull soit terminé (max 25 min par tentative)
        for i in 0..150 {
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;

            // Vérifier via fichiers markers (plus fiable que pgrep)
            match ssh::execute_command_password(host, username, password,
                "if [ -f /tmp/docker_pull_done ]; then echo DONE; elif [ -f /tmp/docker_pull_failed ]; then echo FAILED; elif grep -qi 'failed\\|error\\|timeout' ~/jellysetup-logs/docker_pull.log 2>/dev/null; then echo FAILED; else echo RUNNING; fi"
            ).await {
                Ok(output) => {
                    let output = output.trim();
                    if output.contains("DONE") {
                        println!("[Install] Docker pull marker found, quick validation...");

                        // VÉRIFICATION RAPIDE: Valider que docker-compose.yml est OK (2-5s au lieu de 60s+)
                        let compose_check = ssh::execute_command_password(host, username, password,
                            "cd ~/media-stack && docker compose config >/dev/null 2>&1 && echo OK || echo FAILED"
                        ).await.unwrap_or_default();

                        if compose_check.trim() != "OK" {
                            println!("[Install] Docker compose config validation failed! Will retry pull...");
                            ssh::execute_command_password(host, username, password,
                                "echo \"$(date): Docker compose config validation failed, retrying pull...\" >> ~/jellysetup-logs/install.log"
                            ).await.ok();
                            ssh::execute_command_password(host, username, password,
                                "rm -f /tmp/docker_pull_done"
                            ).await.ok();
                            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                            continue 'pull_loop;  // Réessayer
                        }

                        println!("[Install] Docker compose validated successfully!");
                        ssh::execute_command_password(host, username, password,
                            "echo \"$(date): Docker pull completed and verified - all images present\" >> ~/jellysetup-logs/install.log"
                        ).await.ok();
                        break 'pull_loop;  // Succès, sortir de la boucle principale
                    } else if output.contains("FAILED") {
                        println!("[Install] Docker pull failed, will retry...");
                        ssh::execute_command_password(host, username, password,
                            "echo \"$(date): Docker pull FAILED - retrying...\" >> ~/jellysetup-logs/install.log"
                        ).await.ok();
                        // Attendre 10s avant de réessayer
                        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                        continue 'pull_loop;  // Réessayer
                    }
                    // RUNNING - afficher progression
                    let progress = 60 + (i as u32 * 10 / 150).min(14);
                    emit_progress(&window, "compose_up", progress,
                        &format!("Téléchargement images... (~{}min)", (150 - i) / 6), None);
                }
                Err(_) => {
                    println!("[Install] SSH check failed, retrying...");
                }
            }
        }

        // Timeout atteint sans succès ni échec détecté - considérer comme échec
        println!("[Install] Docker pull timeout, will retry...");
    }

    // Lancer docker compose up - ÉTAPE CRITIQUE
    logger.start_step("docker_compose_up").await;
    emit_progress(&window, "compose_up", 74, "Démarrage des conteneurs...", None);

    let compose_up_result = ssh::execute_command_password(host, username, password,
        "cd ~/media-stack && docker compose up -d 2>&1"
    ).await;

    let compose_up_success = compose_up_result.is_ok();

    match &compose_up_result {
        Ok(output) => {
            // Vérifier si la sortie contient des erreurs même si la commande SSH a réussi
            let output_lower = output.to_lowercase();
            let has_error = output_lower.contains("error") ||
                           output_lower.contains("failed") ||
                           output_lower.contains("cannot") ||
                           output_lower.contains("permission denied");

            if has_error {
                logger.log_error(
                    "docker_compose_up",
                    "docker compose up -d a retourné des erreurs !",
                    Some(serde_json::json!({
                        "output": output.chars().take(1000).collect::<String>(),
                        "command": "cd ~/media-stack && docker compose up -d",
                        "error_detected": true
                    }))
                ).await;
                return Err(anyhow::anyhow!("Docker compose up a échoué: {}", output.chars().take(500).collect::<String>()));
            }

            logger.log_with_details(
                LogLevel::Success,
                "docker_compose_up",
                "docker compose up -d exécuté avec succès",
                serde_json::json!({
                    "output": output.chars().take(500).collect::<String>(),
                    "command": "cd ~/media-stack && docker compose up -d"
                })
            ).await;
        }
        Err(e) => {
            logger.log_error("docker_compose_up", &format!("docker compose up -d FAILED: {}", e), Some(serde_json::json!({
                "error": e.to_string(),
                "command": "cd ~/media-stack && docker compose up -d"
            }))).await;
            return Err(anyhow::anyhow!("Docker compose up a échoué: {}", e));
        }
    }

    // VÉRIFICATION CRITIQUE: S'assurer que les containers tournent VRAIMENT
    // Attendre un peu pour que les containers démarrent
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;

    let containers_check = ssh::execute_command_password(host, username, password,
        "docker ps --format '{{.Names}}: {{.Status}}' 2>&1"
    ).await.unwrap_or_default();

    let container_count = ssh::execute_command_password(host, username, password,
        "docker ps -q | wc -l"
    ).await.unwrap_or_default().trim().parse::<i32>().unwrap_or(0);

    logger.log_with_details(
        LogLevel::Info,
        "docker_compose_up",
        &format!("État des conteneurs après démarrage: {} containers actifs", container_count),
        serde_json::json!({
            "containers": containers_check.trim(),
            "container_count": container_count
        })
    ).await;

    // VÉRIFICATION STRICTE: On attend 9 containers minimum (10 avec Cloudflare)
    let expected_min_containers = 9; // decypharr, jellyfin, radarr, sonarr, prowlarr, jellyseerr, bazarr, flaresolverr, supabazarr

    if container_count < expected_min_containers {
        // Récupérer les logs docker compose pour debug
        let compose_logs = ssh::execute_command_password(host, username, password,
            "cd ~/media-stack && docker compose logs --tail=50 2>&1"
        ).await.unwrap_or_default();

        logger.log_error(
            "docker_compose_up",
            &format!("ERREUR CRITIQUE: Seulement {} containers sur {} attendus !", container_count, expected_min_containers),
            Some(serde_json::json!({
                "docker_ps_output": containers_check.trim(),
                "expected_minimum": expected_min_containers,
                "actual": container_count,
                "compose_logs": compose_logs.chars().take(2000).collect::<String>()
            }))
        ).await;
        logger.end_step("docker_compose_up", false).await;

        // Lister les images manquantes pour aider au debug
        let missing_images = ssh::execute_command_password(host, username, password,
            "cd ~/media-stack && docker compose config --images 2>/dev/null"
        ).await.unwrap_or_default();

        return Err(anyhow::anyhow!(
            "Docker compose up a échoué: seulement {} containers sur {} attendus. Images requises: {}",
            container_count, expected_min_containers, missing_images.trim()
        ));
    }

    logger.end_step("docker_compose_up", true).await;

    // Étape 7: Attendre que les services soient prêts
    emit_progress(&window, "wait_services", 75, "Attente des services...", None);
    tokio::time::sleep(std::time::Duration::from_secs(30)).await;

    // Étape 8: Configuration des services via API
    emit_progress(&window, "config", 85, "Configuration des services...", None);

    // 8.1: Reset Jellyfin MAIS préserver le ServerId pour éviter "Incompatibilité du serveur"
    emit_progress(&window, "config", 86, "Reset Jellyfin pour config propre...", None);
    debug_log("[JELLYFIN] === Reset de Jellyfin avec préservation ServerId ===");

    // 1. Sauvegarder TOUS les fichiers d'identité serveur
    // Le ServerId peut être dans device.txt, deviceid, ou system.xml selon la version
    let backup_cmd = ssh::execute_command_password(host, username, password,
        "cd ~/media-stack && mkdir -p /tmp/jellyfin-backup && \
         find jellyfin -name 'device*' -type f -exec cp {} /tmp/jellyfin-backup/ \\; 2>/dev/null || true && \
         find jellyfin -name 'system.xml' -type f -exec cp {} /tmp/jellyfin-backup/ \\; 2>/dev/null || true && \
         find jellyfin -name 'network.xml' -type f -exec cp {} /tmp/jellyfin-backup/ \\; 2>/dev/null || true && \
         echo 'Backed up:' && ls -la /tmp/jellyfin-backup/ 2>/dev/null || echo 'No files found'"
    ).await.unwrap_or_default();
    debug_log(&format!("[JELLYFIN] Backup result: {}", backup_cmd));

    // 2. Reset Jellyfin: stop, delete, clean
    let reset_cmds = vec![
        "cd ~/media-stack && docker compose stop jellyfin",
        "cd ~/media-stack && docker compose rm -f jellyfin",
        "cd ~/media-stack && rm -rf jellyfin/*",
    ];
    for cmd in &reset_cmds {
        debug_log(&format!("[JELLYFIN] Executing: {}", cmd));
        ssh::execute_command_password(host, username, password, cmd).await.ok();
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }

    // 3. Restaurer les fichiers d'identité serveur AVANT de démarrer Jellyfin
    let restore_result = ssh::execute_command_password(host, username, password,
        "cd ~/media-stack && \
         mkdir -p jellyfin/data jellyfin/config && \
         for f in /tmp/jellyfin-backup/device*; do \
           [ -f \"$f\" ] && cp -f \"$f\" jellyfin/data/ && echo \"Restored $(basename $f) to data/\"; \
         done 2>/dev/null || true && \
         [ -f /tmp/jellyfin-backup/system.xml ] && cp -f /tmp/jellyfin-backup/system.xml jellyfin/config/ && echo 'system.xml restored' || true && \
         [ -f /tmp/jellyfin-backup/network.xml ] && cp -f /tmp/jellyfin-backup/network.xml jellyfin/config/ && echo 'network.xml restored' || true && \
         echo 'Restored files:' && ls -la jellyfin/data/ jellyfin/config/ 2>/dev/null && \
         rm -rf /tmp/jellyfin-backup"
    ).await.unwrap_or_default();
    debug_log(&format!("[JELLYFIN] Restore result: {}", restore_result));

    // 4. Démarrer Jellyfin avec les fichiers d'identité restaurés
    debug_log("[JELLYFIN] Starting Jellyfin with restored identity files...");
    ssh::execute_command_password(host, username, password,
        "cd ~/media-stack && docker compose up -d jellyfin"
    ).await.ok();

    // Attendre que Jellyfin soit prêt après le reset (max 90 sec)
    debug_log("[JELLYFIN] Attente de Jellyfin après reset...");
    emit_progress(&window, "config", 87, "Attente de Jellyfin...", None);

    let mut jellyfin_ready = false;
    for i in 0..18 {
        let check = ssh::execute_command_password(host, username, password,
            "curl -s 'http://localhost:8096/System/Info/Public' 2>/dev/null || echo 'CURL_ERROR'"
        ).await.unwrap_or_default();

        debug_log(&format!("[JELLYFIN] Check {}/18: {}", i + 1, &check[..std::cmp::min(150, check.len())]));

        if check.contains("ServerName") && check.contains("\"StartupWizardCompleted\":false") {
            jellyfin_ready = true;
            debug_log(&format!("[JELLYFIN] Jellyfin prêt, wizard NON complété ({})", i + 1));
            break;
        }
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }

    if !jellyfin_ready {
        debug_log("[JELLYFIN] ERREUR: Jellyfin non disponible après 90 sec!");
    }

    // Variable pour stocker les infos d'auth Jellyfin (pour auto-login frontend)
    let mut final_jellyfin_auth: Option<JellyfinAuth> = None;

    if jellyfin_ready {
        emit_progress(&window, "config", 88, "Configuration Jellyfin...", None);

        // Échapper les caractères spéciaux pour JSON
        let jf_user = config.jellyfin_username.replace("\\", "\\\\").replace("\"", "\\\"");
        let jf_pass = config.jellyfin_password.replace("\\", "\\\\").replace("\"", "\\\"");
        debug_log(&format!("[JELLYFIN] User: {}, Pass: [{}chars]", jf_user, jf_pass.len()));

        // Configuration COMPLÈTE du wizard - ORDRE CORRECT selon gist officiel:
        // 1. Configuration, 2. GET User, 3. POST User, 4. RemoteAccess, 5. Complete
        debug_log("[JELLYFIN] Configuration du wizard startup...");

        // Étape 1: Configuration langue/pays ET nom du serveur EN PREMIER
        let jf_server_name = config.jellyfin_server_name.replace("\\", "\\\\").replace("\"", "\\\"");
        let startup_config_json = format!(
            r#"{{"UICulture":"fr","MetadataCountryCode":"FR","PreferredMetadataLanguage":"fr","ServerName":"{}"}}"#,
            jf_server_name
        );
        let startup_config_cmd = format!(
            "curl -s -X POST 'http://localhost:8096/Startup/Configuration' -H 'Content-Type: application/json' -d '{}'",
            startup_config_json
        );
        let config_result = ssh::execute_command_password(host, username, password, &startup_config_cmd).await.unwrap_or_default();
        debug_log(&format!("[JELLYFIN] 1. Startup/Configuration (ServerName={}): [{}]", jf_server_name, config_result));
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        // Étape 2: GET /Startup/User pour initialiser l'état
        let get_user_cmd = "curl -s 'http://localhost:8096/Startup/User'";
        let get_user_result = ssh::execute_command_password(host, username, password, get_user_cmd).await.unwrap_or_default();
        debug_log(&format!("[JELLYFIN] 2. GET Startup/User: [{}]", get_user_result));
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;

        // Étape 3: POST /Startup/User pour créer l'utilisateur
        let write_json_cmd = format!(
            r#"echo '{{"Name":"{}","Password":"{}"}}' > /tmp/jf_user.json"#,
            jf_user, jf_pass
        );
        ssh::execute_command_password(host, username, password, &write_json_cmd).await.ok();
        let create_user_cmd = "curl -s -X POST 'http://localhost:8096/Startup/User' -H 'Content-Type: application/json' -d @/tmp/jf_user.json";
        debug_log(&format!("[JELLYFIN] 3. POST Startup/User: Creating {}", jf_user));
        let user_result = ssh::execute_command_password(host, username, password, create_user_cmd).await.unwrap_or_default();
        debug_log(&format!("[JELLYFIN] Startup/User result: [{}]", user_result));
        ssh::execute_command_password(host, username, password, "rm -f /tmp/jf_user.json").await.ok();
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        // Étape 4: Activer l'accès distant
        let remote_access_cmd = r#"curl -s -X POST 'http://localhost:8096/Startup/RemoteAccess' -H 'Content-Type: application/json' -d '{"EnableRemoteAccess":true,"EnableAutomaticPortMapping":false}'"#;
        let remote_result = ssh::execute_command_password(host, username, password, remote_access_cmd).await.unwrap_or_default();
        debug_log(&format!("[JELLYFIN] 4. Startup/RemoteAccess: [{}]", remote_result));
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        // Étape 5: Compléter le wizard
        let complete_cmd = "curl -s -X POST 'http://localhost:8096/Startup/Complete'";
        let complete_result = ssh::execute_command_password(host, username, password, complete_cmd).await.unwrap_or_default();
        debug_log(&format!("[JELLYFIN] 5. Startup/Complete: [{}]", complete_result));
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        // Étape 6: Vérifier que le wizard est complété
        let verify_result = ssh::execute_command_password(host, username, password,
            "curl -s 'http://localhost:8096/System/Info/Public'"
        ).await.unwrap_or_default();
        if verify_result.contains("\"StartupWizardCompleted\":true") {
            debug_log("[JELLYFIN] Wizard COMPLÉTÉ avec succès!");
        } else {
            debug_log(&format!("[JELLYFIN] ERREUR: Wizard pas complété! {}", &verify_result[..std::cmp::min(100, verify_result.len())]));
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            ssh::execute_command_password(host, username, password, "curl -s -X POST 'http://localhost:8096/Startup/Complete'").await.ok();
        }

        // Étape 6: S'authentifier pour obtenir un token et créer les bibliothèques
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;

        // Commande auth sur une seule ligne
        let auth_cmd = format!(
            "curl -s -X POST 'http://localhost:8096/Users/AuthenticateByName' -H 'Content-Type: application/json' -H 'X-Emby-Authorization: MediaBrowser Client=\"JellySetup\", Device=\"RaspberryPi\", DeviceId=\"jellysetup-install\", Version=\"1.0.0\"' -d '{{\"Username\":\"{}\",\"Pw\":\"{}\"}}'",
            jf_user, jf_pass
        );
        debug_log(&format!("[JELLYFIN] Auth command: {}", &auth_cmd[..std::cmp::min(150, auth_cmd.len())]));
        let auth_result = ssh::execute_command_password(host, username, password, &auth_cmd).await.unwrap_or_default();
        debug_log(&format!("[JELLYFIN] Auth result: {}", &auth_result[..std::cmp::min(200, auth_result.len())]));

        // Extraire le token et UserId de la réponse JSON
        let mut jellyfin_auth_data: Option<JellyfinAuth> = None;

        if let Some(token_start) = auth_result.find("\"AccessToken\":\"") {
            let token_rest = &auth_result[token_start + 15..];
            if let Some(token_end) = token_rest.find("\"") {
                let jellyfin_token = token_rest[..token_end].to_string();
                println!("[Config] Jellyfin authenticated, creating libraries...");

                // Extraire UserId de la réponse d'auth
                let mut user_id_from_auth = String::new();
                if let Some(user_start) = auth_result.find("\"User\":{") {
                    let user_json = &auth_result[user_start..];
                    if let Some(id_start) = user_json.find("\"Id\":\"") {
                        let id_rest = &user_json[id_start + 6..];
                        if let Some(id_end) = id_rest.find("\"") {
                            user_id_from_auth = id_rest[..id_end].to_string();
                            debug_log(&format!("[JELLYFIN] UserId extracted: {}", user_id_from_auth));
                        }
                    }
                }

                // Récupérer le ServerId depuis /System/Info/Public
                let server_info = ssh::execute_command_password(host, username, password,
                    "curl -s 'http://localhost:8096/System/Info/Public'"
                ).await.unwrap_or_default();
                debug_log(&format!("[JELLYFIN] Server info: {}", &server_info[..std::cmp::min(200, server_info.len())]));

                let mut server_id = String::new();
                if let Some(sid_start) = server_info.find("\"Id\":\"") {
                    let sid_rest = &server_info[sid_start + 6..];
                    if let Some(sid_end) = sid_rest.find("\"") {
                        server_id = sid_rest[..sid_end].to_string();
                        println!("[Config] Jellyfin ServerId: {}", server_id);
                    }
                }

                // Créer la bibliothèque Films avec LibraryOptions.PathInfos (format correct!)
                // Le secret: il FAUT passer PathInfos dans le body JSON sinon la lib n'a pas d'ItemId
                let movies_lib_cmd = format!(
                    "curl -s -X POST 'http://localhost:8096/Library/VirtualFolders?name=Films&collectionType=movies&refreshLibrary=true' -H 'X-Emby-Token: {}' -H 'Content-Type: application/json' -d '{{\"LibraryOptions\":{{\"PathInfos\":[{{\"Path\":\"/mnt/decypharr/movies\"}}]}}}}'",
                    jellyfin_token
                );
                let movies_result = ssh::execute_command_password(host, username, password, &movies_lib_cmd).await.unwrap_or_default();
                debug_log(&format!("[JELLYFIN] Movies library result: {}", movies_result));
                println!("[Config] Jellyfin: Movies library created");

                // Créer la bibliothèque Séries avec LibraryOptions.PathInfos
                let tv_lib_cmd = format!(
                    "curl -s -X POST 'http://localhost:8096/Library/VirtualFolders?name=S%C3%A9ries&collectionType=tvshows&refreshLibrary=true' -H 'X-Emby-Token: {}' -H 'Content-Type: application/json' -d '{{\"LibraryOptions\":{{\"PathInfos\":[{{\"Path\":\"/mnt/decypharr/tv\"}}]}}}}'",
                    jellyfin_token
                );
                let tv_result = ssh::execute_command_password(host, username, password, &tv_lib_cmd).await.unwrap_or_default();
                debug_log(&format!("[JELLYFIN] TV library result: {}", tv_result));
                println!("[Config] Jellyfin: TV Shows library created");

                // Vérifier que les bibliothèques ont bien un ItemId (sinon elles sont invisibles!)
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                let libs_check = ssh::execute_command_password(host, username, password,
                    &format!("curl -s 'http://localhost:8096/Library/VirtualFolders' -H 'X-Emby-Token: {}'", jellyfin_token)
                ).await.unwrap_or_default();
                debug_log(&format!("[JELLYFIN] Libraries check: {}", &libs_check[..std::cmp::min(500, libs_check.len())]));

                // Vérifier que les deux libs ont un ItemId
                let films_ok = libs_check.contains("Films") && libs_check.contains("\"ItemId\"");
                let series_ok = libs_check.contains("ries") && libs_check.matches("\"ItemId\"").count() >= 2;
                if films_ok && series_ok {
                    println!("[Config] Jellyfin: Both libraries created with ItemId - SUCCESS!");
                } else {
                    println!("[Config] Jellyfin: Warning - libraries might not have ItemId: Films={}, Séries={}", films_ok, series_ok);
                }

                // Note: ServerName et langue déjà configurés via /Startup/Configuration
                // NE PAS appeler /System/Configuration ici car ça reset IsStartupWizardCompleted !
                println!("[Config] Jellyfin: Server already configured via Startup API");

                // Configurer les préférences utilisateur en Français (langue UI + sous-titres + audio)
                if !user_id_from_auth.is_empty() {
                    let user_config_cmd = format!(
                        "curl -s -X POST 'http://localhost:8096/Users/{}/Configuration' -H 'X-Emby-Token: {}' -H 'Content-Type: application/json' -d '{{\"SubtitleLanguagePreference\":\"fre\",\"AudioLanguagePreference\":\"fra\"}}'",
                        user_id_from_auth, jellyfin_token
                    );
                    ssh::execute_command_password(host, username, password, &user_config_cmd).await.ok();

                    // Configurer aussi la langue d'affichage (DisplayLanguage) via User Policy
                    let display_lang_cmd = format!(
                        "curl -s -X POST 'http://localhost:8096/Users/{}/Policy' -H 'X-Emby-Token: {}' -H 'Content-Type: application/json' -d '{{\"IsAdministrator\":true,\"EnableAllFolders\":true}}'",
                        user_id_from_auth, jellyfin_token
                    );
                    ssh::execute_command_password(host, username, password, &display_lang_cmd).await.ok();

                    // Forcer la langue française dans system.xml (au cas où le wizard l'aurait pas appliqué)
                    let force_french_cmd = r#"cd ~/media-stack && \
                        if [ -f jellyfin/config/system.xml ]; then \
                            sed -i 's|<UICulture>[^<]*</UICulture>|<UICulture>fr</UICulture>|g' jellyfin/config/system.xml && \
                            sed -i 's|<PreferredMetadataLanguage>[^<]*</PreferredMetadataLanguage>|<PreferredMetadataLanguage>fr</PreferredMetadataLanguage>|g' jellyfin/config/system.xml && \
                            sed -i 's|<MetadataCountryCode>[^<]*</MetadataCountryCode>|<MetadataCountryCode>FR</MetadataCountryCode>|g' jellyfin/config/system.xml && \
                            echo 'French language forced in system.xml'; \
                        fi"#;
                    ssh::execute_command_password(host, username, password, force_french_cmd).await.ok();
                    println!("[Config] Jellyfin: User preferences set to French (UI + subtitles + audio)");
                }

                // Stocker les infos d'authentification pour le frontend
                if !server_id.is_empty() && !user_id_from_auth.is_empty() {
                    jellyfin_auth_data = Some(JellyfinAuth {
                        server_id: server_id.clone(),
                        access_token: jellyfin_token.clone(),
                        user_id: user_id_from_auth.clone(),
                    });
                    println!("[Config] Jellyfin auth data saved for auto-login: ServerId={}, UserId={}", server_id, user_id_from_auth);
                }
            }
        }

        // Sauvegarder jellyfin_auth_data pour l'utiliser à la fin
        // On le stocke dans une variable qui sera utilisée par emit_progress_with_auth
        final_jellyfin_auth = jellyfin_auth_data;
    } else {
        // ERREUR CRITIQUE: Si Jellyfin n'est pas prêt après 2 min, c'est que l'installation a échoué !
        logger.log_error(
            "jellyfin_config",
            "ERREUR CRITIQUE: Jellyfin n'est pas accessible après 2 minutes d'attente !",
            Some(serde_json::json!({
                "timeout_seconds": 120,
                "attempts": 24,
                "expected_url": "http://localhost:8096/health",
                "expected_response": "200"
            }))
        ).await;

        // Vérifier les logs Docker pour comprendre le problème
        let docker_logs = ssh::execute_command_password(host, username, password,
            "docker logs jellyfin 2>&1 | tail -50"
        ).await.unwrap_or_default();

        logger.log_with_details(
            LogLevel::Error,
            "jellyfin_config",
            "Logs Jellyfin pour debug",
            serde_json::json!({
                "docker_logs": docker_logs.chars().take(2000).collect::<String>()
            })
        ).await;

        return Err(anyhow::anyhow!("Jellyfin n'est pas accessible après 2 minutes d'attente. Les containers Docker ne fonctionnent pas correctement."));
    }

    // 8.3: Configurer Decypharr avec AllDebrid
    emit_progress(&window, "config", 89, "Configuration Decypharr...", None);
    if !config.alldebrid_api_key.is_empty() {
        let ad_key = config.alldebrid_api_key.replace("\\", "\\\\").replace("\"", "\\\"");

        // Créer le config.json pour Decypharr
        let decypharr_config = format!(r#"{{
  "port": "8282",
  "qbit": {{
    "port": 8282,
    "username": "",
    "password": "",
    "download_folder": "/mnt/decypharr/qbit/downloads",
    "categories": {{
      "radarr": "/mnt/decypharr/movies",
      "sonarr": "/mnt/decypharr/tv"
    }}
  }},
  "debrids": [
    {{
      "name": "alldebrid",
      "enabled": true,
      "api_key": "{}",
      "folder": "/mnt/decypharr/alldebrid",
      "download_uncached": true
    }}
  ],
  "repair": {{
    "enabled": true,
    "interval": "1h"
  }}
}}"#, ad_key);

        let write_config_cmd = format!(
            "cat > ~/media-stack/decypharr/config.json << 'EOFDECYPHARR'\n{}\nEOFDECYPHARR",
            decypharr_config
        );
        ssh::execute_command_password(host, username, password, &write_config_cmd).await.ok();

        // Redémarrer Decypharr en background (évite les timeouts SSH)
        ssh::execute_command_password(host, username, password,
            "nohup docker restart decypharr > /dev/null 2>&1 &"
        ).await.ok();
        // Attendre quelques secondes pour laisser le restart démarrer
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        debug_log("[DECYPHARR] Config updated with port as string");
        println!("[Config] Decypharr configured with AllDebrid");
    }

    // 8.4: Attendre que Radarr et Sonarr soient prêts
    emit_progress(&window, "config", 91, "Configuration Radarr/Sonarr...", None);
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Récupérer les API keys de Radarr et Sonarr depuis leurs config.xml
    let radarr_api = ssh::execute_command_password(host, username, password,
        "grep -oP '(?<=<ApiKey>)[^<]+' ~/media-stack/radarr/config.xml 2>/dev/null || echo ''"
    ).await.unwrap_or_default().trim().to_string();

    let sonarr_api = ssh::execute_command_password(host, username, password,
        "grep -oP '(?<=<ApiKey>)[^<]+' ~/media-stack/sonarr/config.xml 2>/dev/null || echo ''"
    ).await.unwrap_or_default().trim().to_string();

    let prowlarr_api = ssh::execute_command_password(host, username, password,
        "grep -oP '(?<=<ApiKey>)[^<]+' ~/media-stack/prowlarr/config.xml 2>/dev/null || echo ''"
    ).await.unwrap_or_default().trim().to_string();

    println!("[Config] API Keys - Radarr: {}..., Sonarr: {}..., Prowlarr: {}...",
        radarr_api.chars().take(8).collect::<String>(),
        sonarr_api.chars().take(8).collect::<String>(),
        prowlarr_api.chars().take(8).collect::<String>()
    );

    // =============================================================================
    // MASTER CONFIG - Fetch dynamique depuis Supabase
    // =============================================================================
    emit_progress(&window, "config", 89, "Récupération de la configuration master...", None);
    println!("[MasterConfig] 🔄 Fetching configuration from Supabase...");

    let master_config_opt = crate::master_config::fetch_master_config(Some("streaming")).await.ok().flatten();

    if let Some(master_cfg) = &master_config_opt {
        println!("[MasterConfig] ✅ Master config loaded: {}", master_cfg.id);

        let mut template_vars = crate::template_engine::TemplateVars::new();
        template_vars.set("PI_IP", host);
        template_vars.set("PI_HOSTNAME", &hostname);
        template_vars.set("RADARR_API_KEY", &radarr_api);
        template_vars.set("SONARR_API_KEY", &sonarr_api);
        template_vars.set("PROWLARR_API_KEY", &prowlarr_api);
        template_vars.set("JELLYFIN_USERNAME", &config.jellyfin_username);
        template_vars.set("JELLYFIN_PASSWORD", &config.jellyfin_password);
        template_vars.set("YGG_PASSKEY", config.admin_email.as_deref().unwrap_or(""));
        template_vars.set("ALLDEBRID_API_KEY", &config.alldebrid_api_key);

        if let Some(jf_auth) = &final_jellyfin_auth {
            template_vars.set("JELLYFIN_API_KEY", &jf_auth.access_token);
            template_vars.set("JELLYFIN_SERVER_ID", &jf_auth.server_id);
        } else {
            template_vars.set("JELLYFIN_API_KEY", "PLACEHOLDER");
            template_vars.set("JELLYFIN_SERVER_ID", "PLACEHOLDER");
        }

        emit_progress(&window, "config", 90, "Application des configurations master...", None);

        if let Some(jellyseerr_config) = &master_cfg.jellyseerr_config {
            println!("[MasterConfig] Applying Jellyseerr config...");
            if let Err(e) = crate::services::apply_service_config_password(
                host, username, password, "jellyseerr", jellyseerr_config, &template_vars,
                &config.jellyfin_username,
                &config.jellyfin_password,
                config.admin_email.as_deref().unwrap_or("admin@jellyseerr.local")
            ).await {
                println!("[MasterConfig] ⚠️  Jellyseerr config error: {}", e);
            }
        }

        if let Some(radarr_config) = &master_cfg.radarr_config {
            println!("[MasterConfig] Applying Radarr config...");
            if let Err(e) = crate::services::apply_service_config_password(
                host, username, password, "radarr", radarr_config, &template_vars,
                &config.jellyfin_username,
                &config.jellyfin_password,
                config.admin_email.as_deref().unwrap_or("admin@jellyseerr.local")
            ).await {
                println!("[MasterConfig] ⚠️  Radarr config error: {}", e);
            }
        }

        if let Some(sonarr_config) = &master_cfg.sonarr_config {
            println!("[MasterConfig] Applying Sonarr config...");
            if let Err(e) = crate::services::apply_service_config_password(
                host, username, password, "sonarr", sonarr_config, &template_vars,
                &config.jellyfin_username,
                &config.jellyfin_password,
                config.admin_email.as_deref().unwrap_or("admin@jellyseerr.local")
            ).await {
                println!("[MasterConfig] ⚠️  Sonarr config error: {}", e);
            }
        }

        if let Some(prowlarr_config) = &master_cfg.prowlarr_config {
            println!("[MasterConfig] Applying Prowlarr config...");
            if let Err(e) = crate::services::apply_service_config_password(
                host, username, password, "prowlarr", prowlarr_config, &template_vars,
                &config.jellyfin_username,
                &config.jellyfin_password,
                config.admin_email.as_deref().unwrap_or("admin@jellyseerr.local")
            ).await {
                println!("[MasterConfig] ⚠️  Prowlarr config error: {}", e);
            }
        }

        if let Some(jellyfin_config) = &master_cfg.jellyfin_config {
            println!("[MasterConfig] Applying Jellyfin config...");
            if let Err(e) = crate::services::apply_service_config_password(
                host, username, password, "jellyfin", jellyfin_config, &template_vars,
                &config.jellyfin_username,
                &config.jellyfin_password,
                config.admin_email.as_deref().unwrap_or("admin@jellyseerr.local")
            ).await {
                println!("[MasterConfig] ⚠️  Jellyfin config error: {}", e);
            }
        }

        println!("[MasterConfig] ✅ All service configurations applied from master_config");
    } else {
        println!("[MasterConfig] ⚠️  No master_config found - using default configuration");
    }
    // =============================================================================

    // Ajouter Decypharr comme client de téléchargement à Radarr
    if !radarr_api.is_empty() {
        let radarr_client_cmd = format!(r#"curl -s -X POST 'http://localhost:7878/api/v3/downloadclient' \
            -H 'X-Api-Key: {}' \
            -H 'Content-Type: application/json' \
            -d '{{
                "name": "Decypharr",
                "implementation": "QBittorrent",
                "configContract": "QBittorrentSettings",
                "enable": true,
                "priority": 1,
                "fields": [
                    {{"name": "host", "value": "decypharr"}},
                    {{"name": "port", "value": 8282}},
                    {{"name": "useSsl", "value": false}},
                    {{"name": "movieCategory", "value": "radarr"}}
                ]
            }}'"#, radarr_api);
        let result = ssh::execute_command_password(host, username, password, &radarr_client_cmd).await;
        println!("[Config] Radarr: Decypharr download client result: {:?}", result);
    }

    // Ajouter Decypharr comme client de téléchargement à Sonarr
    if !sonarr_api.is_empty() {
        let sonarr_client_cmd = format!(r#"curl -s -X POST 'http://localhost:8989/api/v3/downloadclient' \
            -H 'X-Api-Key: {}' \
            -H 'Content-Type: application/json' \
            -d '{{
                "name": "Decypharr",
                "implementation": "QBittorrent",
                "configContract": "QBittorrentSettings",
                "enable": true,
                "priority": 1,
                "fields": [
                    {{"name": "host", "value": "decypharr"}},
                    {{"name": "port", "value": 8282}},
                    {{"name": "useSsl", "value": false}},
                    {{"name": "tvCategory", "value": "sonarr"}}
                ]
            }}'"#, sonarr_api);
        let result = ssh::execute_command_password(host, username, password, &sonarr_client_cmd).await;
        println!("[Config] Sonarr: Decypharr download client result: {:?}", result);
    }

    // 8.4b: Ajouter les Root Folders pour Radarr et Sonarr
    if !radarr_api.is_empty() {
        let radarr_root_cmd = format!(r#"curl -s -X POST 'http://localhost:7878/api/v3/rootfolder' \
            -H 'X-Api-Key: {}' \
            -H 'Content-Type: application/json' \
            -d '{{"path": "/mnt/decypharr/movies"}}'"#, radarr_api);
        ssh::execute_command_password(host, username, password, &radarr_root_cmd).await.ok();
        println!("[Config] Radarr: Root folder /mnt/decypharr/movies added");
    }

    if !sonarr_api.is_empty() {
        let sonarr_root_cmd = format!(r#"curl -s -X POST 'http://localhost:8989/api/v3/rootfolder' \
            -H 'X-Api-Key: {}' \
            -H 'Content-Type: application/json' \
            -d '{{"path": "/mnt/decypharr/tv"}}'"#, sonarr_api);
        ssh::execute_command_password(host, username, password, &sonarr_root_cmd).await.ok();
        println!("[Config] Sonarr: Root folder /mnt/decypharr/tv added");
    }

    // 8.5: Configurer Prowlarr avec YGG (si passkey fournie)
    emit_progress(&window, "config", 94, "Configuration Prowlarr...", None);
    if let Some(ref ygg_passkey) = config.ygg_passkey {
        if !ygg_passkey.is_empty() && !prowlarr_api.is_empty() {
            let passkey = ygg_passkey.replace("\\", "\\\\").replace("\"", "\\\"");

            // D'abord, récupérer le schema de l'indexer YGG
            // Puis ajouter l'indexer avec le passkey
            let prowlarr_ygg_cmd = format!(r#"curl -s -X POST 'http://localhost:9696/api/v1/indexer' \
                -H 'X-Api-Key: {}' \
                -H 'Content-Type: application/json' \
                -d '{{
                    "name": "YGGTorrent",
                    "definitionName": "yggtorrent",
                    "implementation": "YggTorrent",
                    "configContract": "YggTorrentSettings",
                    "enable": true,
                    "protocol": "torrent",
                    "priority": 1,
                    "fields": [
                        {{"name": "passkey", "value": "{}"}}
                    ]
                }}'"#, prowlarr_api, passkey);
            ssh::execute_command_password(host, username, password, &prowlarr_ygg_cmd).await.ok();
            println!("[Config] Prowlarr: YGG indexer configured");

            // Ajouter FlareSolverr à Prowlarr
            let flaresolverr_cmd = format!(r#"curl -s -X POST 'http://localhost:9696/api/v1/indexerProxy' \
                -H 'X-Api-Key: {}' \
                -H 'Content-Type: application/json' \
                -d '{{
                    "name": "FlareSolverr",
                    "configContract": "FlareSolverrSettings",
                    "implementation": "FlareSolverr",
                    "fields": [
                        {{"name": "host", "value": "http://localhost:8191"}}
                    ]
                }}'"#, prowlarr_api);
            ssh::execute_command_password(host, username, password, &flaresolverr_cmd).await.ok();
        }
    }

    // 8.6: Synchroniser Prowlarr avec Radarr et Sonarr
    if !prowlarr_api.is_empty() {
        emit_progress(&window, "config", 96, "Synchronisation Prowlarr...", None);

        // Ajouter Radarr comme application dans Prowlarr
        if !radarr_api.is_empty() {
            let sync_radarr_cmd = format!(r#"curl -s -X POST 'http://localhost:9696/api/v1/applications' \
                -H 'X-Api-Key: {}' \
                -H 'Content-Type: application/json' \
                -d '{{
                    "name": "Radarr",
                    "syncLevel": "fullSync",
                    "implementation": "Radarr",
                    "configContract": "RadarrSettings",
                    "fields": [
                        {{"name": "prowlarrUrl", "value": "http://localhost:9696"}},
                        {{"name": "baseUrl", "value": "http://localhost:7878"}},
                        {{"name": "apiKey", "value": "{}"}}
                    ]
                }}'"#, prowlarr_api, radarr_api);
            ssh::execute_command_password(host, username, password, &sync_radarr_cmd).await.ok();
            println!("[Config] Prowlarr: Radarr sync configured");
        }

        // Ajouter Sonarr comme application dans Prowlarr
        if !sonarr_api.is_empty() {
            let sync_sonarr_cmd = format!(r#"curl -s -X POST 'http://localhost:9696/api/v1/applications' \
                -H 'X-Api-Key: {}' \
                -H 'Content-Type: application/json' \
                -d '{{
                    "name": "Sonarr",
                    "syncLevel": "fullSync",
                    "implementation": "Sonarr",
                    "configContract": "SonarrSettings",
                    "fields": [
                        {{"name": "prowlarrUrl", "value": "http://localhost:9696"}},
                        {{"name": "baseUrl", "value": "http://localhost:8989"}},
                        {{"name": "apiKey", "value": "{}"}}
                    ]
                }}'"#, prowlarr_api, sonarr_api);
            ssh::execute_command_password(host, username, password, &sync_sonarr_cmd).await.ok();
            println!("[Config] Prowlarr: Sonarr sync configured");
        }
    }

    // 8.7: Configurer Bazarr avec Radarr et Sonarr
    emit_progress(&window, "config", 97, "Configuration Bazarr...", None);
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Attendre que Bazarr génère son config.ini
    let mut bazarr_ready = false;
    for _ in 0..12 {
        let check = ssh::execute_command_password(host, username, password,
            "test -f ~/media-stack/bazarr/config/config.yaml && echo OK || echo WAIT"
        ).await.unwrap_or_default();
        if check.contains("OK") {
            bazarr_ready = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }

    if bazarr_ready && !radarr_api.is_empty() && !sonarr_api.is_empty() {
        // Bazarr utilise config.yaml depuis les versions récentes
        // On peut modifier directement les settings via son API après le premier démarrage
        let bazarr_api_check = ssh::execute_command_password(host, username, password,
            "grep -oP '(?<=apikey: )[^\\s]+' ~/media-stack/bazarr/config/config.yaml 2>/dev/null || echo ''"
        ).await.unwrap_or_default().trim().to_string();

        if !bazarr_api_check.is_empty() {
            // Configurer Radarr dans Bazarr
            let bazarr_radarr_cmd = format!(r#"curl -s -X POST 'http://localhost:6767/api/system/settings' \
                -H 'X-API-KEY: {}' \
                -H 'Content-Type: application/json' \
                -d '{{"settings": {{"radarr": {{"ip": "localhost", "port": 7878, "apikey": "{}", "ssl": false, "base_url": ""}}}}}}"#,
                bazarr_api_check, radarr_api);
            ssh::execute_command_password(host, username, password, &bazarr_radarr_cmd).await.ok();

            // Configurer Sonarr dans Bazarr
            let bazarr_sonarr_cmd = format!(r#"curl -s -X POST 'http://localhost:6767/api/system/settings' \
                -H 'X-API-KEY: {}' \
                -H 'Content-Type: application/json' \
                -d '{{"settings": {{"sonarr": {{"ip": "localhost", "port": 8989, "apikey": "{}", "ssl": false, "base_url": ""}}}}}}"#,
                bazarr_api_check, sonarr_api);
            ssh::execute_command_password(host, username, password, &bazarr_sonarr_cmd).await.ok();
            println!("[Config] Bazarr: Radarr and Sonarr configured");
        }
    }

    // 8.8: Configuration automatique de Jellyseerr via API
    emit_progress(&window, "config", 96, "Configuration de Jellyseerr...", None);
    println!("[Config] Jellyseerr: Starting automatic configuration...");

    // Attendre que Jellyseerr soit prêt (max 60 sec)
    let mut jellyseerr_ready = false;
    for i in 0..12 {
        let check = ssh::execute_command_password(host, username, password,
            "curl -s -o /dev/null -w '%{http_code}' 'http://localhost:5055/api/v1/status' 2>/dev/null || echo '000'"
        ).await.unwrap_or_default();

        if check.trim() == "200" || check.trim() == "403" {
            jellyseerr_ready = true;
            println!("[Config] Jellyseerr: Service ready after {} seconds", (i + 1) * 5);
            break;
        }
        println!("[Config] Jellyseerr: Waiting... (attempt {}/12)", i + 1);
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }

    if jellyseerr_ready {
        // Petite pause pour s'assurer que Jellyseerr est complètement prêt
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;

        // Étape 1: Authentifier avec Jellyfin et créer l'admin
        // IMPORTANT: Échapper les caractères spéciaux pour JSON
        let jf_user = config.jellyfin_username.replace("\\", "\\\\").replace("\"", "\\\"");
        let jf_pass = config.jellyfin_password.replace("\\", "\\\\").replace("\"", "\\\"");

        // Essayer plusieurs hostnames jusqu'à ce qu'un fonctionne
        // 1. host.docker.internal (avec extra_hosts configuré)
        // 2. jellyfin (nom du service Docker sur le même réseau)
        // 3. IP du Pi (passée en paramètre host)
        let hostnames_to_try = vec![
            "host.docker.internal".to_string(),
            "jellyfin".to_string(),
            host.to_string(),
        ];

        let mut auth_result = String::new();
        for jellyfin_hostname in &hostnames_to_try {
            println!("[Config] Jellyseerr: Trying hostname: {}", jellyfin_hostname);
            // serverType: 2 = JELLYFIN (enum MediaServerType)
            // urlBase: "" évite que JavaScript ajoute "undefined" à l'URL
            let auth_cmd = format!(
                r#"curl -s -X POST 'http://localhost:5055/api/v1/auth/jellyfin' \
                   -H 'Content-Type: application/json' \
                   -c /tmp/jellyseerr_cookies.txt \
                   -d '{{"username":"{}","password":"{}","hostname":"{}","port":8096,"useSsl":false,"urlBase":"","serverType":2,"email":"admin@easyjelly.local"}}'"#,
                jf_user, jf_pass, jellyfin_hostname
            );
            auth_result = ssh::execute_command_password(host, username, password, &auth_cmd).await.unwrap_or_default();
            println!("[Config] Jellyseerr: Auth result with {}: {}", jellyfin_hostname, &auth_result[..std::cmp::min(200, auth_result.len())]);

            if auth_result.contains("\"id\"") {
                println!("[Config] Jellyseerr: Success with hostname: {}", jellyfin_hostname);
                break;
            }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }

        // Vérifier si l'auth a réussi (contient "id" dans la réponse)
        if auth_result.contains("\"id\"") {
            println!("[Config] Jellyseerr: Admin user created successfully!");

            // Étape 2: Sync des bibliothèques Jellyfin
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            let sync_cmd = "curl -s -X GET 'http://localhost:5055/api/v1/settings/jellyfin/library?sync=true' -b /tmp/jellyseerr_cookies.txt";
            let sync_result = ssh::execute_command_password(host, username, password, sync_cmd).await.unwrap_or_default();
            println!("[Config] Jellyseerr: Library sync result: {}", &sync_result[..std::cmp::min(300, sync_result.len())]);

            // Extraire les IDs des bibliothèques (format: [{"id":"xxx","name":"Films",...}])
            let mut library_ids: Vec<String> = Vec::new();
            let mut search_pos = 0;
            while let Some(id_start) = sync_result[search_pos..].find("\"id\":\"") {
                let actual_start = search_pos + id_start + 6;
                if let Some(id_end) = sync_result[actual_start..].find("\"") {
                    let lib_id = &sync_result[actual_start..actual_start + id_end];
                    library_ids.push(lib_id.to_string());
                    search_pos = actual_start + id_end;
                } else {
                    break;
                }
            }

            // Étape 3: Activer toutes les bibliothèques trouvées
            if !library_ids.is_empty() {
                let ids_str = library_ids.join(",");
                let enable_cmd = format!(
                    "curl -s -X GET 'http://localhost:5055/api/v1/settings/jellyfin/library?enable={}' -b /tmp/jellyseerr_cookies.txt",
                    ids_str
                );
                ssh::execute_command_password(host, username, password, &enable_cmd).await.ok();
                println!("[Config] Jellyseerr: Enabled {} libraries: {}", library_ids.len(), ids_str);
            }

            // Étape 4: Finaliser le setup
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            let init_cmd = "curl -s -X POST 'http://localhost:5055/api/v1/settings/initialize' -b /tmp/jellyseerr_cookies.txt -H 'Content-Type: application/json'";
            let init_result = ssh::execute_command_password(host, username, password, init_cmd).await.unwrap_or_default();
            println!("[Config] Jellyseerr: Initialize result: {}", init_result);

            // Nettoyer les cookies
            ssh::execute_command_password(host, username, password, "rm -f /tmp/jellyseerr_cookies.txt").await.ok();

            println!("[Config] Jellyseerr: Configuration completed successfully!");
        } else {
            println!("[Config] Jellyseerr: Auth failed, manual setup required at http://<pi-ip>:5055");
        }
    } else {
        println!("[Config] Jellyseerr: Service not ready after 60 seconds, manual setup required");
    }

    // Log la configuration effectuée
    ssh::execute_command_password(host, username, password,
        "echo \"$(date): Service configuration completed\" >> ~/jellysetup-logs/install.log"
    ).await.ok();

    // 8.9: Sauvegarder l'installation dans Supabase (centralisation des identifiants)
    emit_progress(&window, "supabase", 98, "Sauvegarde dans le cloud...", None);

    // Récupérer le fingerprint SSH capturé au début
    let ssh_fingerprint = ssh::get_last_host_fingerprint();

    // Sauvegarder dans Supabase (ne bloque pas en cas d'erreur)
    match crate::supabase::save_installation(
        &hostname,
        host,
        None,  // Pas de clé publique pour auth par mot de passe
        None,  // Pas de clé privée pour auth par mot de passe
        ssh_fingerprint.as_deref(),
        env!("CARGO_PKG_VERSION"),
    ).await {
        Ok(config_id) => {
            println!("[Supabase] Installation saved with ID: {}", config_id);

            // Sauvegarder aussi les credentials de l'utilisateur
            if let Err(e) = crate::supabase::save_pi_config(
                &hostname,
                &config_id,
                Some(&config.alldebrid_api_key),
                config.ygg_passkey.as_deref(),
                config.cloudflare_token.as_deref(),
                None, // jellyfin_api_key
                None, // radarr_api_key
                None, // sonarr_api_key
                None, // prowlarr_api_key
            ).await {
                println!("[Supabase] Warning: could not save Pi config: {}", e);
            }

            // Mettre à jour le statut à "completed"
            if let Err(e) = crate::supabase::update_status(&hostname, &config_id, "completed", None).await {
                println!("[Supabase] Warning: could not update status: {}", e);
            }
        }
        Err(e) => {
            println!("[Supabase] Warning: could not save installation: {}", e);
        }
    }

    // Émettre l'événement de fin avec les données d'auth Jellyfin pour auto-login
    emit_progress_with_auth(&window, "complete", 100, "Installation terminée !", None, final_jellyfin_auth);

    // Finaliser les logs et envoyer tout à Supabase
    logger.finalize(true).await;

    // Arrêter caffeinate maintenant que l'installation est terminée
    #[cfg(target_os = "macos")]
    if let Some(mut process) = caffeinate_process {
        println!("[Install] Stopping caffeinate...");
        let _ = process.kill();
    }

    // Fermer la session SSH persistante
    ssh::close_persistent_session().await;

    tracing::info!("Installation (password auth) completed successfully on {}", host);
    Ok(())
}
