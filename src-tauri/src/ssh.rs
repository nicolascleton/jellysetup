use anyhow::{anyhow, Result};
use russh::*;
use russh_keys::*;
use std::sync::Arc;

struct Client {}

#[async_trait::async_trait]
impl client::Handler for Client {
    type Error = anyhow::Error;

    async fn check_server_key(
        self,
        _server_public_key: &russh_keys::key::PublicKey,
    ) -> std::result::Result<(Self, bool), Self::Error> {
        // Accepter toutes les clés (première connexion)
        // En production, on devrait vérifier le fingerprint
        Ok((self, true))
    }
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

    let config = client::Config::default();
    let config = Arc::new(config);

    let mut session = match client::connect(config, (host, 22), Client {}).await {
        Ok(s) => s,
        Err(e) => {
            println!("[SSH] Connection failed: {}", e);
            return Err(anyhow!("Connection failed: {}", e));
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
    let config = client::Config::default();
    let config = Arc::new(config);

    let key = russh_keys::decode_secret_key(private_key, None)?;

    let mut session = client::connect(config, (host, 22), Client {}).await?;

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

    let config = client::Config::default();
    let config = Arc::new(config);

    let mut session = match client::connect(config, (host, 22), Client {}).await {
        Ok(s) => {
            println!("[SSH] exec_password: connected");
            s
        }
        Err(e) => {
            println!("[SSH] exec_password: connection failed: {}", e);
            return Err(anyhow!("Connection failed: {}", e));
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
