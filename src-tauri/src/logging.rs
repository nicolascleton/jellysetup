// =============================================================================
// MEGA SYSTÈME DE LOGS - Multi-tenant pour JellySetup
// =============================================================================
// Ce module gère les logs d'installation avec:
// - Logs locaux sur le Pi (~/jellysetup-logs/)
// - Logs Supabase dans le schéma dédié au Pi
// - Support batch pour performance optimale
// - Niveaux de log: DEBUG, INFO, WARN, ERROR, SUCCESS, CRITICAL
// =============================================================================

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;
use uuid::Uuid;

// =============================================================================
// TYPES ET STRUCTURES
// =============================================================================

/// Niveaux de log supportés
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
    Success,
    Critical,
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogLevel::Debug => write!(f, "DEBUG"),
            LogLevel::Info => write!(f, "INFO"),
            LogLevel::Warn => write!(f, "WARN"),
            LogLevel::Error => write!(f, "ERROR"),
            LogLevel::Success => write!(f, "SUCCESS"),
            LogLevel::Critical => write!(f, "CRITICAL"),
        }
    }
}

/// Une entrée de log complète
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp: DateTime<Utc>,
    pub level: LogLevel,
    pub step: String,
    pub substep: Option<String>,
    pub message: String,
    pub details: Option<serde_json::Value>,
    pub duration_ms: Option<i64>,
    pub progress_percent: Option<i32>,
    pub ssh_command: Option<String>,
    pub ssh_output: Option<String>,
    pub ssh_exit_code: Option<i32>,
    pub installer_version: Option<String>,
    pub session_id: Option<String>,
    pub tags: Vec<String>,
}

impl LogEntry {
    pub fn new(level: LogLevel, step: &str, message: &str) -> Self {
        Self {
            timestamp: Utc::now(),
            level,
            step: step.to_string(),
            substep: None,
            message: message.to_string(),
            details: None,
            duration_ms: None,
            progress_percent: None,
            ssh_command: None,
            ssh_output: None,
            ssh_exit_code: None,
            installer_version: None,
            session_id: None,
            tags: vec![],
        }
    }

    pub fn with_substep(mut self, substep: &str) -> Self {
        self.substep = Some(substep.to_string());
        self
    }

    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }

    pub fn with_duration(mut self, duration_ms: i64) -> Self {
        self.duration_ms = Some(duration_ms);
        self
    }

    pub fn with_progress(mut self, percent: i32) -> Self {
        self.progress_percent = Some(percent);
        self
    }

    pub fn with_ssh(mut self, command: &str, output: &str, exit_code: i32) -> Self {
        self.ssh_command = Some(command.to_string());
        self.ssh_output = Some(output.to_string());
        self.ssh_exit_code = Some(exit_code);
        self
    }

    pub fn with_session(mut self, session_id: &str) -> Self {
        self.session_id = Some(session_id.to_string());
        self
    }

    pub fn with_tags(mut self, tags: Vec<&str>) -> Self {
        self.tags = tags.iter().map(|s| s.to_string()).collect();
        self
    }
}

// =============================================================================
// INSTALLATION LOGGER - Logger principal pour une installation
// =============================================================================

/// Logger pour une installation spécifique
pub struct InstallationLogger {
    /// Nom du Pi (utilisé pour le schéma Supabase)
    pub pi_name: String,
    /// IP du Pi (pour les logs locaux)
    pub pi_ip: String,
    /// Hôte SSH
    pub ssh_host: String,
    /// Username SSH
    pub ssh_username: String,
    /// Password SSH
    pub ssh_password: String,
    /// Session ID unique pour cette installation
    pub session_id: String,
    /// Version de l'installateur
    pub installer_version: String,
    /// Buffer de logs en attente d'envoi
    log_buffer: Arc<Mutex<Vec<LogEntry>>>,
    /// Timer pour mesurer les durées
    step_timer: Arc<Mutex<Option<Instant>>>,
    /// Étape courante
    current_step: Arc<Mutex<String>>,
}

impl InstallationLogger {
    /// Crée un nouveau logger pour une installation
    pub fn new(
        pi_name: &str,
        pi_ip: &str,
        ssh_host: &str,
        ssh_username: &str,
        ssh_password: &str,
        installer_version: &str,
    ) -> Self {
        Self {
            pi_name: pi_name.to_string(),
            pi_ip: pi_ip.to_string(),
            ssh_host: ssh_host.to_string(),
            ssh_username: ssh_username.to_string(),
            ssh_password: ssh_password.to_string(),
            session_id: Uuid::new_v4().to_string(),
            installer_version: installer_version.to_string(),
            log_buffer: Arc::new(Mutex::new(Vec::new())),
            step_timer: Arc::new(Mutex::new(None)),
            current_step: Arc::new(Mutex::new(String::new())),
        }
    }

    /// Initialise le système de logs (crée le dossier local + schéma Supabase)
    pub async fn initialize(&self) -> Result<()> {
        // 1. Créer le dossier de logs sur le Pi
        let init_cmd = format!(
            "mkdir -p ~/jellysetup-logs && echo '{}' > ~/jellysetup-logs/session_id.txt",
            self.session_id
        );

        if let Err(e) = crate::ssh::execute_command_password(
            &self.ssh_host,
            &self.ssh_username,
            &self.ssh_password,
            &init_cmd,
        ).await {
            println!("[Logger] Warning: could not create log dir on Pi: {}", e);
        }

        // 2. Initialiser le schéma Supabase
        if let Err(e) = crate::supabase::ensure_schema_initialized(&self.pi_name).await {
            println!("[Logger] Warning: could not init Supabase schema: {}", e);
        }

        // 3. Logger le début de session
        self.log(LogLevel::Info, "session_start", &format!(
            "Installation started - Session: {} - Pi: {} ({})",
            self.session_id, self.pi_name, self.pi_ip
        )).await;

        Ok(())
    }

    /// Démarre le timer pour une étape
    pub async fn start_step(&self, step: &str) {
        let mut timer = self.step_timer.lock().await;
        *timer = Some(Instant::now());

        let mut current = self.current_step.lock().await;
        *current = step.to_string();

        self.log(LogLevel::Info, step, &format!("Starting: {}", step)).await;
    }

    /// Termine une étape et retourne la durée en ms
    pub async fn end_step(&self, step: &str, success: bool) -> i64 {
        let timer = self.step_timer.lock().await;
        let duration_ms = timer.map(|t| t.elapsed().as_millis() as i64).unwrap_or(0);

        let level = if success { LogLevel::Success } else { LogLevel::Error };
        let status = if success { "completed" } else { "failed" };

        let entry = LogEntry::new(level, step, &format!("{}: {} ({}ms)", step, status, duration_ms))
            .with_duration(duration_ms)
            .with_session(&self.session_id);

        self.log_entry(entry).await;

        duration_ms
    }

    /// Log un message simple
    pub async fn log(&self, level: LogLevel, step: &str, message: &str) {
        let entry = LogEntry::new(level, step, message)
            .with_session(&self.session_id);
        self.log_entry(entry).await;
    }

    /// Log avec détails JSON
    pub async fn log_with_details(&self, level: LogLevel, step: &str, message: &str, details: serde_json::Value) {
        let entry = LogEntry::new(level, step, message)
            .with_details(details)
            .with_session(&self.session_id);
        self.log_entry(entry).await;
    }

    /// Log une commande SSH avec son résultat
    pub async fn log_ssh(&self, step: &str, command: &str, output: &str, exit_code: i32) {
        let level = if exit_code == 0 { LogLevel::Debug } else { LogLevel::Error };
        let entry = LogEntry::new(level, step, &format!("SSH command: {}", command))
            .with_ssh(command, output, exit_code)
            .with_session(&self.session_id);
        self.log_entry(entry).await;
    }

    /// Log une erreur avec contexte
    pub async fn log_error(&self, step: &str, error: &str, details: Option<serde_json::Value>) {
        let mut entry = LogEntry::new(LogLevel::Error, step, error)
            .with_session(&self.session_id)
            .with_tags(vec!["error", "needs_attention"]);

        if let Some(d) = details {
            entry = entry.with_details(d);
        }

        self.log_entry(entry).await;
    }

    /// Log une entrée complète
    pub async fn log_entry(&self, mut entry: LogEntry) {
        // Ajouter les métadonnées
        entry.installer_version = Some(self.installer_version.clone());
        if entry.session_id.is_none() {
            entry.session_id = Some(self.session_id.clone());
        }

        // Afficher dans la console
        let emoji = match entry.level {
            LogLevel::Debug => "🔍",
            LogLevel::Info => "ℹ️",
            LogLevel::Warn => "⚠️",
            LogLevel::Error => "❌",
            LogLevel::Success => "✅",
            LogLevel::Critical => "🚨",
        };
        println!("{} [{}] [{}] {}", emoji, entry.level, entry.step, entry.message);

        // Log local sur le Pi (non-bloquant)
        let local_log = format!(
            "[{}] [{}] [{}] {}\n",
            entry.timestamp.format("%Y-%m-%d %H:%M:%S"),
            entry.level,
            entry.step,
            entry.message
        );

        let ssh_host = self.ssh_host.clone();
        let ssh_user = self.ssh_username.clone();
        let ssh_pass = self.ssh_password.clone();

        tokio::spawn(async move {
            let cmd = format!(
                "echo '{}' >> ~/jellysetup-logs/install.log",
                local_log.replace("'", "'\\''")
            );
            crate::ssh::execute_command_password(&ssh_host, &ssh_user, &ssh_pass, &cmd).await.ok();
        });

        // Ajouter au buffer pour envoi batch à Supabase
        let mut buffer = self.log_buffer.lock().await;
        buffer.push(entry);

        // Flush si le buffer est assez grand
        if buffer.len() >= 5 {
            drop(buffer);
            self.flush_to_supabase().await;
        }
    }

    /// Envoie les logs en attente à Supabase
    pub async fn flush_to_supabase(&self) {
        let mut buffer = self.log_buffer.lock().await;
        if buffer.is_empty() {
            return;
        }

        let logs: Vec<LogEntry> = buffer.drain(..).collect();
        drop(buffer);

        // Envoyer à Supabase via l'Edge Function SÉCURISÉE (clé ANON uniquement)
        let client = reqwest::Client::new();
        let supabase_url = crate::supabase::get_supabase_url_public();
        // SÉCURITÉ: On utilise la clé ANON (publique) et PAS la SERVICE_KEY
        // L'Edge Function jellysetup-logs vérifie le token et utilise ses propres droits
        let anon_key = crate::supabase::get_supabase_anon_key();

        let body = json!({
            "logs": logs.iter().map(|l| json!({
                "level": l.level.to_string(),
                "step": l.step,
                "substep": l.substep,
                "message": l.message,
                "details": l.details,
                "duration_ms": l.duration_ms,
                "progress_percent": l.progress_percent,
                "ssh_command": l.ssh_command,
                "ssh_output": l.ssh_output,
                "ssh_exit_code": l.ssh_exit_code,
                "installer_version": l.installer_version,
                "session_id": l.session_id,
                "tags": l.tags,
            })).collect::<Vec<_>>()
        });

        // Utiliser le hostname (pi_name) pour le schéma, pas l'IP
        let schema_name = self.pi_name.to_lowercase()
            .chars()
            .map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' })
            .collect::<String>();

        // Nouvelle Edge Function sécurisée qui accepte la clé ANON
        let url = format!("{}/functions/v1/jellysetup-logs?hostname={}", supabase_url, schema_name);
        println!("[Logger] Sending {} logs to: {}", logs.len(), url);

        match client
            .post(&url)
            .header("Authorization", format!("Bearer {}", anon_key))
            .header("Content-Type", "application/json")
            .header("X-Pi-Hostname", &self.pi_name)
            .json(&body)
            .send()
            .await
        {
            Ok(response) => {
                let status = response.status();
                if status.is_success() {
                    println!("[Logger] ✅ Logs sent successfully ({} logs)", logs.len());
                } else {
                    let error_text = response.text().await.unwrap_or_default();
                    println!("[Logger] ❌ Supabase returned error {}: {}", status, error_text);
                }
            }
            Err(e) => {
                println!("[Logger] ❌ Network error sending logs: {}", e);
            }
        }
    }

    /// Finalise et envoie tous les logs restants
    pub async fn finalize(&self, success: bool) {
        let status = if success { "completed" } else { "failed" };
        self.log(
            if success { LogLevel::Success } else { LogLevel::Error },
            "session_end",
            &format!("Installation {} - Session: {}", status, self.session_id)
        ).await;

        self.flush_to_supabase().await;
    }
}

// =============================================================================
// MACROS UTILITAIRES
// =============================================================================

/// Macro pour logger facilement
#[macro_export]
macro_rules! log_info {
    ($logger:expr, $step:expr, $($arg:tt)*) => {
        $logger.log($crate::logging::LogLevel::Info, $step, &format!($($arg)*)).await
    };
}

#[macro_export]
macro_rules! log_error {
    ($logger:expr, $step:expr, $($arg:tt)*) => {
        $logger.log($crate::logging::LogLevel::Error, $step, &format!($($arg)*)).await
    };
}

#[macro_export]
macro_rules! log_success {
    ($logger:expr, $step:expr, $($arg:tt)*) => {
        $logger.log($crate::logging::LogLevel::Success, $step, &format!($($arg)*)).await
    };
}

#[macro_export]
macro_rules! log_debug {
    ($logger:expr, $step:expr, $($arg:tt)*) => {
        $logger.log($crate::logging::LogLevel::Debug, $step, &format!($($arg)*)).await
    };
}

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

/// Exécute une commande SSH et log automatiquement le résultat
pub async fn execute_and_log(
    logger: &InstallationLogger,
    step: &str,
    command: &str,
) -> Result<String> {
    let start = Instant::now();

    match crate::ssh::execute_command_password(
        &logger.ssh_host,
        &logger.ssh_username,
        &logger.ssh_password,
        command,
    ).await {
        Ok(output) => {
            let duration = start.elapsed().as_millis() as i64;
            logger.log_with_details(
                LogLevel::Debug,
                step,
                &format!("Command succeeded ({}ms)", duration),
                json!({
                    "command": command,
                    "output_length": output.len(),
                    "duration_ms": duration
                })
            ).await;
            Ok(output)
        }
        Err(e) => {
            logger.log_error(step, &format!("Command failed: {}", e), Some(json!({
                "command": command,
                "error": e.to_string()
            }))).await;
            Err(e)
        }
    }
}

/// Exécute une commande SSH, log le résultat, et retourne aussi le code de sortie
pub async fn execute_and_log_full(
    logger: &InstallationLogger,
    step: &str,
    command: &str,
) -> (Result<String>, i32) {
    let start = Instant::now();

    // On va parser le code de sortie depuis la commande
    let wrapped_cmd = format!("{}; echo \"EXIT_CODE:$?\"", command);

    match crate::ssh::execute_command_password(
        &logger.ssh_host,
        &logger.ssh_username,
        &logger.ssh_password,
        &wrapped_cmd,
    ).await {
        Ok(output) => {
            let duration = start.elapsed().as_millis() as i64;

            // Extraire le code de sortie
            let (actual_output, exit_code) = if let Some(idx) = output.rfind("EXIT_CODE:") {
                let code_str = output[idx + 10..].trim();
                let code = code_str.parse::<i32>().unwrap_or(-1);
                (output[..idx].trim().to_string(), code)
            } else {
                (output.clone(), 0)
            };

            logger.log_ssh(step, command, &actual_output, exit_code).await;

            if exit_code == 0 {
                (Ok(actual_output), exit_code)
            } else {
                (Err(anyhow::anyhow!("Command exited with code {}", exit_code)), exit_code)
            }
        }
        Err(e) => {
            logger.log_error(step, &format!("SSH error: {}", e), Some(json!({
                "command": command,
                "error": e.to_string()
            }))).await;
            (Err(e), -1)
        }
    }
}
