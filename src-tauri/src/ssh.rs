use anyhow::{anyhow, Result};
use russh::*;
use russh_keys::*;
use std::sync::Arc;
use std::sync::Mutex;
use once_cell::sync::Lazy;

// Stockage temporaire du dernier fingerprint capturé
static LAST_HOST_FINGERPRINT: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));

struct Client {}

#[async_trait::async_trait]
impl client::Handler for Client {
    type Error = anyhow::Error;

    async fn check_server_key(
        self,
        server_public_key: &russh_keys::key::PublicKey,
    ) -> std::result::Result<(Self, bool), Self::Error> {
        // Capturer le fingerprint SHA256 de la clé host
        let fingerprint = server_public_key.fingerprint();
        println!("[SSH] Host fingerprint: {}", fingerprint);

        // Stocker le fingerprint pour récupération ultérieure
        if let Ok(mut fp) = LAST_HOST_FINGERPRINT.lock() {
            *fp = Some(fingerprint);
        }

        // Accepter toutes les clés (on vérifie/stocke le fingerprint dans Supabase)
        Ok((self, true))
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

    // ssh-keygen -R <ip>
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

/// Teste la connexion SSH avec clé privée
pub async fn test_connection(host: &str, username: &str, private_key: &str) -> Result<bool> {
    let config = client::Config::default();
    let config = Arc::new(config);

    let key = russh_keys::decode_secret_key(private_key, None)?;

    let mut session = client::connect(config, (host, 22), Client {}).await?;

    let auth_result = session
        .authenticate_publickey(username, Arc::new(key))
        .await?;

    session.disconnect(Disconnect::ByApplication, "", "").await?;

    Ok(auth_result)
}

/// Teste la connexion SSH avec mot de passe
pub async fn test_connection_password(host: &str, username: &str, password: &str) -> Result<bool> {
    println!("[SSH] Connecting to {}@{}...", username, host);

    // Retry logic pour gérer "No route to host" transitoire
    let mut session = None;
    let mut last_error = None;

    for attempt in 1..=3 {
        let config = client::Config::default();
        let config = Arc::new(config);

        match client::connect(config, (host, 22), Client {}).await {
            Ok(s) => {
                println!("[SSH] test_connection: connected (attempt {})", attempt);
                session = Some(s);
                break;
            }
            Err(e) => {
                println!("[SSH] test_connection: failed (attempt {}): {}", attempt, e);
                last_error = Some(e);
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

    // Retry logic pour gérer "No route to host" transitoire
    let mut session = None;
    let mut last_error = None;

    for attempt in 1..=3 {
        let config = client::Config::default();
        let config = Arc::new(config);

        match client::connect(config, (host, 22), Client {}).await {
            Ok(s) => {
                println!("[SSH] execute_command: connected (attempt {})", attempt);
                session = Some(s);
                break;
            }
            Err(e) => {
                println!("[SSH] execute_command: connection failed (attempt {}): {}", attempt, e);
                last_error = Some(e);
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
pub async fn execute_command_password(
    host: &str,
    username: &str,
    password: &str,
    command: &str,
) -> Result<String> {
    println!("[SSH] exec_password: connecting to {}@{}", username, host);
    println!("[SSH] Command: {}", &command[..command.len().min(100)]);

    // Retry logic pour gérer "No route to host" transitoire
    let mut session = None;
    let mut last_error = None;

    for attempt in 1..=3 {
        let config = client::Config::default();
        let config = Arc::new(config);

        match client::connect(config, (host, 22), Client {}).await {
            Ok(s) => {
                println!("[SSH] exec_password: connected (attempt {})", attempt);
                session = Some(s);
                break;
            }
            Err(e) => {
                println!("[SSH] exec_password: connection failed (attempt {}): {}", attempt, e);
                last_error = Some(e);
                if attempt < 3 {
                    // Attendre 2 secondes avant de réessayer
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
    let mut channel = session.channel_open_session().await?;

    channel.exec(true, command).await?;

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

    channel.eof().await?;
    session.disconnect(Disconnect::ByApplication, "", "").await?;

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
    // Pour l'instant, on utilise une commande SSH avec cat
    let escaped_content = local_content.replace("'", "'\\''");
    let command = format!("cat > {} << 'JELLYSETUP_EOF'\n{}\nJELLYSETUP_EOF", remote_path, escaped_content);

    execute_command(host, username, private_key, &command).await?;

    Ok(())
}
