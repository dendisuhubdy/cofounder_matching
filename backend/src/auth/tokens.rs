use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use rand::RngCore;
use sha2::{Digest, Sha256};

/// A 256-bit cryptographically random token, safe to place in a URL.
pub fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// SHA-256 of the token. Only this is persisted, so a database leak does not
/// hand over usable login links or sessions.
///
/// A password KDF would be the wrong tool here: the input is already 256 bits
/// of entropy, so there is nothing to brute-force and nothing for key
/// stretching to protect.
pub fn hash_token(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}
