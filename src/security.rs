use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::Rng;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789";

pub fn paste_id(length: usize) -> String {
    let mut rng = rand::rng();
    (0..length)
        .map(|_| ALPHABET[rng.random_range(0..ALPHABET.len())] as char)
        .collect()
}

pub fn delete_token() -> String {
    let bytes: [u8; 32] = rand::random();
    URL_SAFE_NO_PAD.encode(bytes)
}

pub fn hash_token(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    URL_SAFE_NO_PAD.encode(digest)
}

pub fn verify_token(token: &str, hash: &str) -> bool {
    if token.len() != 43 {
        return false;
    }
    let candidate = hash_token(token);
    bool::from(candidate.as_bytes().ct_eq(hash.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::valid_id;

    #[test]
    fn ids_are_random_and_valid() {
        let ids: std::collections::HashSet<_> = (0..1000).map(|_| paste_id(8)).collect();
        assert_eq!(ids.len(), 1000);
        assert!(ids.iter().all(|id| valid_id(id)));
    }

    #[test]
    fn tokens_verify() {
        let token = delete_token();
        let hash = hash_token(&token);
        assert!(verify_token(&token, &hash));
        assert!(!verify_token(&delete_token(), &hash));
        assert!(!hash.contains(&token));
    }
}
