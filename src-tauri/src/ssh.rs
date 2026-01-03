use anyhow::{anyhow, Result};
use russh::*;
use russh_keys::*;
use std::sync::Arc;
use std::sync::Mutex;
use once_cell::sync::Lazy;
use tokio::sync::Mutex as TokioMutex;

// Stockage temporaire du dernier fingerprint capturé
static LAST_HOST_FINGERPRINT: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));

// Session SSH persistante globale
static PERSISTENT_SESSION: Lazy<TokioMutex<Option<PersistentSession>>> =
    Lazy::new(|| TokioMutex::new(None));

struct Client {}

#[async_trait::async_trait]
impl client::Handler for Client {
    type Error = anyhow::Error;

    async fn check_server_key(
        self,
        server_public_key: &russh_keys::key::PublicKey,
    ) -> std::result::Result<(Self, bool), Self::Error> {
        let fingerprint = server_public_key.fingerprint();

        if let Ok(mut fp) = LAST_HOST_FINGERPRINT.lock() {
            *fp = Some(fingerprint);
        }

        Ok((self, true))
    }
}

/// Structure pour gérer une session SSH persistante
struct PersistentSession {
    host: String,
    username: String,
    password: String,
    session: client::Handle<Client>,
    command_count: u32,
}

impl PersistentSession {
    /// Crée une nouvelle session persistante
    async fn new(host: &str, username: &str, password: &str) -> Result<Self> {
        println!("[SSH-PERSISTENT] Creating new persistent session to {}@{}", username, host);

        let config = Arc::new(client::Config::default());

        let mut session = match tokio::time::timeout(
            std::time::Duration::from_secs(15),
            client::connect(config, (host, 22), Client {})
        ).await {
            Ok(Ok(s)) => s,
            Ok(Err(e)) => return Err(anyhow!("Connection failed: {}", e)),
            Err(_) => return Err(anyhow!("Connection timeout")),
        };

        let auth_result = session.authenticate_password(username, password).await?;
        if !auth_result {
            return Err(anyhow!("Authentication failed"));
        }

        println!("[SSH-PERSISTENT] ✅ Session established and authenticated");

        Ok(Self {
            host: host.to_string(),
            username: username.to_string(),
            password: password.to_string(),
            session,
            command_count: 0,
        })
    }

    /// Exécute une commande sur la session persistante
    async fn exec(&mut self, command: &str) -> Result<String> {
        self.command_count += 1;

        // Log court pour les commandes
        let cmd_preview = if command.len() > 60 {
            format!("{}...", &command[..60])
        } else {
            command.to_string()
        };
        println!("[SSH-P #{}] {}", self.command_count, cmd_preview);

        // Ouvrir un channel pour cette commande - timeout court pour fail fast
        let mut channel = match tokio::time::timeout(
            std::time::Duration::from_secs(5),
            self.session.channel_open_session()
        ).await {
            Ok(Ok(ch)) => ch,
            Ok(Err(e)) => {
                println!("[SSH-P] Channel failed: {}", e);
                return Err(anyhow!("Channel open failed: {}", e));
            }
            Err(_) => {
                println!("[SSH-P] Channel timeout");
                return Err(anyhow!("Channel open timeout"));
            }
        };

        if let Err(e) = channel.exec(true, command).await {
            return Err(anyhow!("Command exec failed: {}", e));
        }

        let mut output = String::new();

        loop {
            match channel.wait().await {
                Some(ChannelMsg::Data { data }) => {
                    output.push_str(&String::from_utf8_lossy(&data));
                }
                Some(ChannelMsg::ExtendedData { data, .. }) => {
                    output.push_str(&String::from_utf8_lossy(&data));
                }
                Some(ChannelMsg::ExitStatus { exit_status }) => {
                    if exit_status != 0 {
                        tracing::warn!("Command exited with status {}: {}", exit_status, output);
                    }
                    break;
                }
                Some(ChannelMsg::Eof) => break,
                None => break,
                _ => {}
            }
        }

        // Fermer proprement le channel
        let _ = channel.eof().await;
        let _ = channel.close().await;

        // Attendre un peu pour que le channel se ferme complètement
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        Ok(output)
    }

    /// Vérifie si la session est valide
    async fn is_alive(&mut self) -> bool {
        match self.exec("echo ok").await {
            Ok(out) => out.trim() == "ok",
            Err(_) => false,
        }
    }
}

/// Récupère le dernier fingerprint SSH host capturé
pub fn get_last_host_fingerprint() -> Option<String> {
    LAST_HOST_FINGERPRINT.lock().ok().and_then(|fp| fp.clone())
}

/// Nettoie le known_hosts local pour une IP donnée
pub fn clear_known_hosts_for_ip(ip: &str) -> Result<()> {
    use std::process::Command;

    println!("[SSH] Clearing known_hosts entry for {}...", ip);

    let output = Command::new("ssh-keygen")
        .args(["-R", ip])
        .output()?;

    if output.status.success() {
        println!("[SSH] Cleared known_hosts entry for {}", ip);
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        println!("[SSH] Warning clearing known_hosts: {}", stderr);
    }

    Ok(())
}

/// Initialise ou réutilise une session SSH persistante
pub async fn init_persistent_session(host: &str, username: &str, password: &str) -> Result<()> {
    let mut session_guard = PERSISTENT_SESSION.lock().await;

    // Vérifier si on a déjà une session valide pour ce host
    if let Some(ref mut existing) = *session_guard {
        if existing.host == host && existing.username == username {
            // Vérifier que la session est encore vivante
            if existing.is_alive().await {
                println!("[SSH-PERSISTENT] Reusing existing session ({} commands executed)", existing.command_count);
                return Ok(());
            } else {
                println!("[SSH-PERSISTENT] Existing session is dead, recreating...");
            }
        } else {
            println!("[SSH-PERSISTENT] Different host/user, creating new session");
        }
    }

    // Créer une nouvelle session
    let new_session = PersistentSession::new(host, username, password).await?;
    *session_guard = Some(new_session);

    Ok(())
}

/// Exécute une commande via la session persistante (avec password)
pub async fn exec_persistent(command: &str) -> Result<String> {
    let mut session_guard = PERSISTENT_SESSION.lock().await;

    if let Some(ref mut session) = *session_guard {
        match session.exec(command).await {
            Ok(output) => return Ok(output),
            Err(e) => {
                println!("[SSH-PERSISTENT] Command failed, session might be dead: {}", e);
                // La session est morte, on la supprime
                *session_guard = None;
                return Err(anyhow!("Session dead: {}", e));
            }
        }
    }

    Err(anyhow!("No persistent session available. Call init_persistent_session first."))
}

/// Ferme la session persistante
pub async fn close_persistent_session() {
    let mut session_guard = PERSISTENT_SESSION.lock().await;
    if session_guard.is_some() {
        println!("[SSH-PERSISTENT] Closing persistent session");
        *session_guard = None;
    }
}

/// Teste la connexion SSH avec clé privée
pub async fn test_connection(host: &str, username: &str, private_key: &str) -> Result<bool> {
    let config = Arc::new(client::Config::default());

    let key = russh_keys::decode_secret_key(private_key, None)?;

    let mut session = match tokio::time::timeout(
        std::time::Duration::from_secs(15),
        client::connect(config, (host, 22), Client {})
    ).await {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => return Err(anyhow!("Connection failed: {}", e)),
        Err(_) => return Err(anyhow!("Connection timeout after 15s")),
    };

    let auth_result = session
        .authenticate_publickey(username, Arc::new(key))
        .await?;

    session.disconnect(Disconnect::ByApplication, "", "").await?;

    Ok(auth_result)
}

/// Teste la connexion SSH avec mot de passe
pub async fn test_connection_password(host: &str, username: &str, password: &str) -> Result<bool> {
    println!("[SSH] Connecting to {}@{}...", username, host);

    let mut session = None;
    let mut last_error = None;

    for attempt in 1..=3 {
        let config = Arc::new(client::Config::default());

        match tokio::time::timeout(
            std::time::Duration::from_secs(15),
            client::connect(config, (host, 22), Client {})
        ).await {
            Ok(Ok(s)) => {
                println!("[SSH] test_connection: connected (attempt {})", attempt);
                session = Some(s);
                break;
            }
            Ok(Err(e)) => {
                println!("[SSH] test_connection: connection failed (attempt {}): {}", attempt, e);
                last_error = Some(anyhow!("{}", e));
                if attempt < 3 {
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                }
            }
            Err(_) => {
                println!("[SSH] test_connection: timeout (attempt {})", attempt);
                last_error = Some(anyhow!("Connection timeout after 15s"));
                if attempt < 3 {
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                }
            }
        }
    }

    let mut session = match session {
        Some(s) => s,
        None => {
            return Err(anyhow!("Connection failed after 3 attempts: {}", last_error.unwrap()));
        }
    };

    println!("[SSH] Authenticating with password...");
    let auth_result = match session.authenticate_password(username, password).await {
        Ok(r) => r,
        Err(e) => {
            println!("[SSH] Password auth failed: {}", e);
            return Err(anyhow!("Password auth failed: {}", e));
        }
    };

    println!("[SSH] Auth result: {}", auth_result);

    if let Err(e) = session.disconnect(Disconnect::ByApplication, "", "").await {
        println!("[SSH] Disconnect warning: {}", e);
    }

    Ok(auth_result)
}

/// Exécute une commande SSH et retourne la sortie (clé privée)
pub async fn execute_command(
    host: &str,
    username: &str,
    private_key: &str,
    command: &str,
) -> Result<String> {
    let key = russh_keys::decode_secret_key(private_key, None)?;

    let mut session = None;
    let mut last_error = None;

    for attempt in 1..=3 {
        let config = Arc::new(client::Config::default());

        match tokio::time::timeout(
            std::time::Duration::from_secs(15),
            client::connect(config, (host, 22), Client {})
        ).await {
            Ok(Ok(s)) => {
                println!("[SSH] execute_command: connected (attempt {})", attempt);
                session = Some(s);
                break;
            }
            Ok(Err(e)) => {
                println!("[SSH] execute_command: connection failed (attempt {}): {}", attempt, e);
                last_error = Some(anyhow!("{}", e));
                if attempt < 3 {
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                }
            }
            Err(_) => {
                println!("[SSH] execute_command: timeout (attempt {})", attempt);
                last_error = Some(anyhow!("Connection timeout after 15s"));
                if attempt < 3 {
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                }
            }
        }
    }

    let mut session = match session {
        Some(s) => s,
        None => {
            return Err(anyhow!("Connection failed after 3 attempts: {:?}", last_error));
        }
    };

    let auth_result = session
        .authenticate_publickey(username, Arc::new(key))
        .await?;

    if !auth_result {
        return Err(anyhow!("Authentication failed"));
    }

    execute_on_session(&mut session, command).await
}

/// Exécute une commande SSH et retourne la sortie (mot de passe)
/// Utilise la session persistante si disponible, sinon en crée une nouvelle
pub async fn execute_command_password(
    host: &str,
    username: &str,
    password: &str,
    command: &str,
) -> Result<String> {
    // Essayer d'utiliser la session persistante si disponible
    {
        let mut session_guard = PERSISTENT_SESSION.lock().await;
        if let Some(ref mut session) = *session_guard {
            if session.host == host && session.username == username {
                // Timeout de 60s pour les commandes via session persistante
                match tokio::time::timeout(
                    std::time::Duration::from_secs(60),
                    session.exec(command)
                ).await {
                    Ok(Ok(output)) => return Ok(output),
                    Ok(Err(e)) => {
                        println!("[SSH] Persistent session command failed: {}", e);
                        // Réinitialiser la session
                        *session_guard = None;
                    }
                    Err(_) => {
                        println!("[SSH] Persistent session timeout, reconnecting...");
                        *session_guard = None;
                    }
                }

                // Essayer de reconnecter automatiquement
                drop(session_guard);
                if let Ok(()) = init_persistent_session(host, username, password).await {
                    // Réessayer avec la nouvelle session
                    let mut session_guard = PERSISTENT_SESSION.lock().await;
                    if let Some(ref mut session) = *session_guard {
                        match session.exec(command).await {
                            Ok(output) => return Ok(output),
                            Err(e) => {
                                println!("[SSH] Reconnected session also failed: {}", e);
                                *session_guard = None;
                            }
                        }
                    }
                }
            }
        }
    }

    // Fallback: créer une nouvelle connexion
    println!("[SSH] exec_password: connecting to {}@{}", username, host);
    println!("[SSH] Command: {}", &command[..command.len().min(100)]);

    let mut session = None;
    let mut last_error = None;

    for attempt in 1..=3 {
        let config = Arc::new(client::Config::default());

        match tokio::time::timeout(
            std::time::Duration::from_secs(15),
            client::connect(config, (host, 22), Client {})
        ).await {
            Ok(Ok(s)) => {
                println!("[SSH] exec_password: connected (attempt {})", attempt);
                session = Some(s);
                break;
            }
            Ok(Err(e)) => {
                println!("[SSH] exec_password: connection failed (attempt {}): {}", attempt, e);
                last_error = Some(anyhow!("{}", e));
                if attempt < 3 {
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                }
            }
            Err(_) => {
                println!("[SSH] exec_password: timeout (attempt {})", attempt);
                last_error = Some(anyhow!("Connection timeout after 15s"));
                if attempt < 3 {
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                }
            }
        }
    }

    let mut session = match session {
        Some(s) => s,
        None => {
            return Err(anyhow!("Connection failed after 3 attempts: {:?}", last_error));
        }
    };

    println!("[SSH] exec_password: authenticating...");
    let auth_result = match session.authenticate_password(username, password).await {
        Ok(r) => r,
        Err(e) => {
            println!("[SSH] exec_password: auth failed: {}", e);
            return Err(anyhow!("Password auth failed: {}", e));
        }
    };

    if !auth_result {
        println!("[SSH] exec_password: auth returned false");
        return Err(anyhow!("Password authentication failed"));
    }

    println!("[SSH] exec_password: executing command...");
    execute_on_session(&mut session, command).await
}

/// Fonction interne pour exécuter une commande sur une session
async fn execute_on_session(
    session: &mut client::Handle<Client>,
    command: &str,
) -> Result<String> {
    println!("[SSH] Opening channel...");
    let mut channel = match tokio::time::timeout(
        std::time::Duration::from_secs(30),
        session.channel_open_session()
    ).await {
        Ok(Ok(ch)) => ch,
        Ok(Err(e)) => return Err(anyhow!("Channel open failed: {}", e)),
        Err(_) => return Err(anyhow!("Channel open timeout after 30s")),
    };

    println!("[SSH] Executing command...");
    if let Err(e) = channel.exec(true, command).await {
        return Err(anyhow!("Command exec failed: {}", e));
    }

    let mut output = String::new();

    loop {
        match channel.wait().await {
            Some(ChannelMsg::Data { data }) => {
                output.push_str(&String::from_utf8_lossy(&data));
            }
            Some(ChannelMsg::ExtendedData { data, .. }) => {
                output.push_str(&String::from_utf8_lossy(&data));
            }
            Some(ChannelMsg::ExitStatus { exit_status }) => {
                if exit_status != 0 {
                    tracing::warn!("Command exited with status {}: {}", exit_status, output);
                }
                break;
            }
            Some(ChannelMsg::Eof) => break,
            None => break,
            _ => {}
        }
    }

    let _ = channel.eof().await;
    let _ = session.disconnect(Disconnect::ByApplication, "", "").await;

    Ok(output)
}

/// Exécute plusieurs commandes en séquence
pub async fn execute_commands(
    host: &str,
    username: &str,
    private_key: &str,
    commands: &[&str],
) -> Result<Vec<String>> {
    let mut results = Vec::new();

    for cmd in commands {
        let output = execute_command(host, username, private_key, cmd).await?;
        results.push(output);
    }

    Ok(results)
}

/// Upload un fichier via SFTP
pub async fn upload_file(
    host: &str,
    username: &str,
    private_key: &str,
    local_content: &str,
    remote_path: &str,
) -> Result<()> {
    let escaped_content = local_content.replace("'", "'\\''");
    let command = format!("cat > {} << 'JELLYSETUP_EOF'\n{}\nJELLYSETUP_EOF", remote_path, escaped_content);

    execute_command(host, username, private_key, &command).await?;

    Ok(())
}
