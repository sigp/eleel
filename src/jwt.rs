//! JWT authentication supporting multiple secrets identified by ID.
use crate::config::ClientJwtSecrets;
use hmac::{Hmac, Mac};
use jwt::{Error, Header, Token, Unverified, Verified, VerifyWithKey};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::collections::HashMap;
use std::path::Path;

pub type VerifiedToken = Token<Header, Claims, Verified>;
pub type UnverifiedToken<'a> = Token<Header, Claims, Unverified<'a>>;
pub type Secret = Hmac<Sha256>;

/// Collection of JWT secrets organised by ID, allowing for each client to use its own secret.
pub struct KeyCollection {
    secrets: HashMap<String, Secret>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct Claims {
    /// issued-at claim. Represented as seconds passed since UNIX_EPOCH.
    iat: u64,
    /// Optional unique identifier for the CL node.
    id: Option<String>,
    /// Optional client version for the CL node.
    clv: Option<String>,
}

pub fn verify_single_token(token: &str, secret: &Secret) -> Result<VerifiedToken, String> {
    token.verify_with_key(secret).map_err(convert_err)
}

fn verify_parsed_token(token: UnverifiedToken, secret: &Secret) -> Result<VerifiedToken, String> {
    token.verify_with_key(secret).map_err(convert_err)
}

pub fn jwt_secret_from_path(path: &Path) -> Result<Secret, String> {
    std::fs::read_to_string(path)
        .map_err(|e| e.to_string())
        .and_then(|hex_secret| hex::decode(hex_secret).map_err(|e| e.to_string()))
        .and_then(|byte_secret| Secret::new_from_slice(&byte_secret).map_err(|e| e.to_string()))
        .map_err(|e| format!("Invalid JWT secret at path {}: {e}", path.display()))
}

impl KeyCollection {
    pub fn verify(&self, token: &str) -> Result<VerifiedToken, String> {
        let parsed_token = UnverifiedToken::parse_unverified(token).map_err(convert_err)?;

        // Look up the key by ID. Unlike other JWT implementations, the engine API puts the key ID
        // inside the claim.
        let secret = parsed_token
            .claims()
            .id
            .as_ref()
            .and_then(|id| Some((id, self.secrets.get(id)?)));

        if let Some((id, secret)) = secret {
            tracing::trace!(id = id, "matched JWT secret by ID");
            return verify_parsed_token(parsed_token, secret);
        }

        // Otherwise try every token available (slow).
        // TODO: put this behind a CLI flag once more CL clients support key IDs
        for (id, secret) in &self.secrets {
            if let Ok(token) = verify_single_token(token, secret) {
                tracing::trace!(id = id, "matched JWT secret by iteration");
                return Ok(token);
            }
        }

        // No matching key found.
        Err("No matching JWT secret found".into())
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let raw = ClientJwtSecrets::from_file(path)?;

        let mut secrets = HashMap::with_capacity(raw.secrets.len());

        for (id, hex_secret) in raw.secrets {
            let byte_secret =
                hex::decode(&hex_secret).map_err(|e| format!("Invalid JWT secret: {e:?}"))?;

            let secret = Secret::new_from_slice(&byte_secret)
                .map_err(|e| format!("Invalid JWT secret: {e}"))?;
            secrets.insert(id, secret);
        }

        Ok(Self { secrets })
    }
}

fn convert_err(e: Error) -> String {
    format!("JWT verification error: {e}")
}
