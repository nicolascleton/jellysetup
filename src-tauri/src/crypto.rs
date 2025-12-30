use crate::SSHCredentials;
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use anyhow::Result;
use argon2::{password_hash::SaltString, Argon2, PasswordHasher};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use rand::{rngs::OsRng, RngCore};
use russh_keys::key::KeyPair;

/// Génère une paire de clés SSH Ed25519
pub async fn generate_ssh_keypair() -> Result<SSHCredentials> {
    // Générer une paire de clés Ed25519
    let keypair = russh_keys::key::KeyPair::generate_ed25519()
        .ok_or_else(|| anyhow::anyhow!("Failed to generate key pair"))?;

    // Formater la clé publique en format OpenSSH
    let public_key = format_public_key(&keypair)?;

    // Formater la clé privée en format OpenSSH
    let private_key = format_private_key(&keypair)?;

    Ok(SSHCredentials {
        public_key,
        private_key,
    })
}

/// Formate la clé publique en format OpenSSH
fn format_public_key(keypair: &KeyPair) -> Result<String> {
    let public_key = keypair.clone_public_key()?;
    let mut buffer = Vec::new();
    russh_keys::write_public_key_base64(&mut buffer, &public_key)?;
    let key_str = String::from_utf8(buffer)?;
    Ok(format!("{} jellysetup@pi", key_str.trim()))
}

/// Formate la clé privée en format OpenSSH PEM
fn format_private_key(keypair: &KeyPair) -> Result<String> {
    let mut buffer = Vec::new();
    russh_keys::encode_pkcs8_pem(keypair, &mut buffer)?;
    Ok(String::from_utf8(buffer)?)
}

/// Chiffre la clé privée avec un mot de passe admin
pub fn encrypt_private_key(private_key: &str, admin_password: &str) -> Result<String> {
    // Générer un sel aléatoire
    let salt = SaltString::generate(&mut OsRng);

    // Dériver la clé avec Argon2
    let argon2 = Argon2::default();
    let password_hash = argon2
        .hash_password(admin_password.as_bytes(), &salt)?
        .to_string();

    // Extraire le hash (les 32 premiers bytes)
    let hash_bytes = password_hash.as_bytes();
    let key_bytes: [u8; 32] = hash_bytes[..32].try_into()?;

    // Générer un nonce aléatoire
    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    // Chiffrer avec AES-256-GCM
    let cipher = Aes256Gcm::new_from_slice(&key_bytes)?;
    let ciphertext = cipher
        .encrypt(nonce, private_key.as_bytes())
        .map_err(|e| anyhow::anyhow!("Encryption failed: {}", e))?;

    // Combiner: salt (22 chars) + nonce (12 bytes) + ciphertext
    let mut combined = Vec::new();
    combined.extend_from_slice(salt.as_str().as_bytes());
    combined.extend_from_slice(&nonce_bytes);
    combined.extend_from_slice(&ciphertext);

    // Encoder en base64
    Ok(BASE64.encode(&combined))
}

/// Déchiffre la clé privée (côté admin seulement)
pub fn decrypt_private_key(encrypted: &str, admin_password: &str) -> Result<String> {
    // Décoder le base64
    let combined = BASE64.decode(encrypted)?;

    if combined.len() < 34 {
        return Err(anyhow::anyhow!("Invalid encrypted data"));
    }

    // Extraire les composants
    let salt_str = std::str::from_utf8(&combined[..22])?;
    let salt = SaltString::from_b64(salt_str)?;
    let nonce_bytes: [u8; 12] = combined[22..34].try_into()?;
    let ciphertext = &combined[34..];

    // Dériver la clé avec le même sel
    let argon2 = Argon2::default();
    let password_hash = argon2
        .hash_password(admin_password.as_bytes(), &salt)?
        .to_string();

    let hash_bytes = password_hash.as_bytes();
    let key_bytes: [u8; 32] = hash_bytes[..32].try_into()?;

    // Déchiffrer
    let cipher = Aes256Gcm::new_from_slice(&key_bytes)?;
    let nonce = Nonce::from_slice(&nonce_bytes);

    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| anyhow::anyhow!("Decryption failed: {}", e))?;

    Ok(String::from_utf8(plaintext)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_generate_keypair() {
        let result = generate_ssh_keypair().await;
        assert!(result.is_ok());

        let creds = result.unwrap();
        assert!(creds.public_key.contains("ssh-ed25519"));
        assert!(creds.private_key.contains("-----BEGIN"));
    }

    #[test]
    fn test_encrypt_decrypt() {
        let private_key = "test-private-key-content";
        let password = "super-secret-admin-password";

        let encrypted = encrypt_private_key(private_key, password).unwrap();
        let decrypted = decrypt_private_key(&encrypted, password).unwrap();

        assert_eq!(private_key, decrypted);
    }
}
