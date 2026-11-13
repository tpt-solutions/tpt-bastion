// SPDX-License-Identifier: Apache-2.0 OR MIT

//! At-rest encryption for state files.
//!
//! State is encrypted with AES-256-GCM (authenticated encryption). The symmetric
//! key is *never* stored: it is derived from a user-supplied passphrase using
//! Argon2id, salted with a per-file random salt. Each encryption uses a fresh
//! random 96-bit nonce, so identical plaintexts encrypt to different ciphertexts.

use crate::error::StateError;
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use argon2::{
    password_hash::{PasswordHasher, SaltString},
    Argon2,
};
use rand::rngs::OsRng;
use rand::RngCore;
use sha2::Sha256;

/// Length in bytes of the per-file salt feeding Argon2id.
pub const SALT_LEN: usize = 16;

/// Length in bytes of the AES-GCM nonce (96 bits, as required by the cipher).
pub const NONCE_LEN: usize = 12;

/// Argon2id work factors. Tuned for interactive use on a single document;
/// these are deliberately reproducible so the same passphrase+salt always
/// yields the same key.
const ARGON2_MEMORY_KIB: u32 = 65_536; // 64 MiB
const ARGON2_TIME_COST: u32 = 3;
const ARGON2_PARALLELISM: u32 = 1;

/// A fully-derived, zeroizing symmetric key and the parameters used to derive
/// it. The key is wiped from memory when dropped.
pub struct DerivedKey {
    key: zeroize::Zeroizing<Vec<u8>>,
    salt: Vec<u8>,
}

impl DerivedKey {
    /// Derive a key from a passphrase and a fresh random salt.
    pub fn from_passphrase(passphrase: &str) -> Result<Self, StateError> {
        let mut salt = vec![0u8; SALT_LEN];
        OsRng.fill_bytes(&mut salt);
        Self::from_passphrase_and_salt(passphrase, &salt)
    }

    /// Derive a key from a passphrase and an explicit salt (used when
    /// decrypting an existing file whose salt is stored in the envelope).
    pub fn from_passphrase_and_salt(passphrase: &str, salt: &[u8]) -> Result<Self, StateError> {
        let salt_string =
            SaltString::encode_b64(salt).map_err(|e| StateError::KeyDerivation(e.to_string()))?;
        let argon2 = Argon2::new(
            argon2::Algorithm::Argon2id,
            argon2::Version::V0x13,
            argon2::Params::new(
                ARGON2_MEMORY_KIB,
                ARGON2_TIME_COST,
                ARGON2_PARALLELISM,
                Some(32),
            )
            .map_err(|e| StateError::KeyDerivation(e.to_string()))?,
        );
        let hash = argon2
            .hash_password(passphrase.as_bytes(), &salt_string)
            .map_err(|e| StateError::KeyDerivation(e.to_string()))?
            .hash
            .ok_or_else(|| StateError::KeyDerivation("argon2 produced no hash".into()))?;
        let raw = hash.as_bytes().to_vec();
        Ok(DerivedKey {
            key: zeroize::Zeroizing::new(raw),
            salt: salt.to_vec(),
        })
    }

    /// The salt used for this derivation; stored in the envelope so the same
    /// passphrase reproduces the key on load.
    pub fn salt(&self) -> &[u8] {
        &self.salt
    }

    /// Encrypt `plaintext`, returning `(nonce, ciphertext)`. A fresh nonce is
    /// generated on every call.
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<(Vec<u8>, Vec<u8>), StateError> {
        let mut nonce = vec![0u8; NONCE_LEN];
        OsRng.fill_bytes(&mut nonce);
        let cipher = Aes256Gcm::new_from_slice(&self.key)
            .map_err(|e| StateError::Envelope(e.to_string()))?;
        let ct = cipher
            .encrypt(Nonce::from_slice(&nonce), plaintext)
            .map_err(|_| StateError::Decrypt)?;
        Ok((nonce, ct))
    }

    /// Decrypt `ciphertext` that was produced with `nonce`.
    pub fn decrypt(&self, nonce: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, StateError> {
        let cipher = Aes256Gcm::new_from_slice(&self.key)
            .map_err(|e| StateError::Envelope(e.to_string()))?;
        cipher
            .decrypt(Nonce::from_slice(nonce), ciphertext)
            .map_err(|_| StateError::Decrypt)
    }
}

/// A stable, non-cryptographic fingerprint of an arbitrary byte slice, used to
/// detect whether two state blobs differ. Implemented over SHA-256 so the value
/// is stable across processes and machines (unlike `DefaultHasher`).
pub fn fingerprint(bytes: &[u8]) -> String {
    use sha2::Digest;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let out = hasher.finalize();
    let mut s = String::with_capacity(out.len() * 2);
    for b in out {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// Encode bytes as a lowercase hex string. Used for the salt/nonce/ciphertext
/// fields of the encrypted envelope so the on-disk JSON stays human-inspectable.
pub fn to_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// Decode a hex string back into bytes. Returns an error string on odd length or
/// invalid digits (surfaced as [`StateError::Envelope`]).
pub fn from_hex(s: &str) -> Result<Vec<u8>, StateError> {
    if s.len() % 2 != 0 {
        return Err(StateError::Envelope("hex string has odd length".into()));
    }
    let mut out = Vec::with_capacity(s.len() / 2);
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let hi = hex_val(bytes[i])?;
        let lo = hex_val(bytes[i + 1])?;
        out.push((hi << 4) | lo);
        i += 2;
    }
    Ok(out)
}

fn hex_val(c: u8) -> Result<u8, StateError> {
    match c {
        b'0'..=b'9' => Ok(c - b'0'),
        b'a'..=b'f' => Ok(c - b'a' + 10),
        b'A'..=b'F' => Ok(c - b'A' + 10),
        _ => Err(StateError::Envelope("invalid hex digit".into())),
    }
}
