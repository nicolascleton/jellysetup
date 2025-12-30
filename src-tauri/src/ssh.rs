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

/// Teste la connexion SSH
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

/// Exécute une commande SSH et retourne la sortie
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
