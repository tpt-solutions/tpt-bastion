// SPDX-License-Identifier: Apache-2.0 OR MIT

//! On-disk state backend with optional encryption and cooperative locking.

use crate::backend::StateBackend;
use crate::crypto::DerivedKey;
use crate::error::StateError;
use crate::history::StateHistory;
use crate::lock::{acquire_lock_file, release_lock_file, LockGuard};
use crate::state::{State, STATE_FORMAT_VERSION};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// KDF tag recorded in the encrypted envelope.
const KDF_TAG: &str = "argon2id";

/// The on-disk envelope. Plaintext stores the JSON state inline; encrypted
/// stores the hex-encoded salt/nonce/ciphertext produced by [`DerivedKey`].
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(tag = "format")]
enum Envelope {
    #[serde(rename = "bastion-state-plain-v1")]
    Plain { data: String },
    #[serde(rename = "bastion-state-enc-v1")]
    Encrypted {
        kdf: String,
        salt: String,
        nonce: String,
        ciphertext: String,
    },
}

/// A [`StateBackend`] backed by a single file on disk.
///
/// Construct with [`FileStateBackend::plain`] for unencrypted storage or
/// [`FileStateBackend::encrypted`] for at-rest encryption. Both support the
/// cooperative lock and append-only history stored alongside the state file.
pub struct FileStateBackend {
    path: PathBuf,
    passphrase: Option<String>,
}

impl FileStateBackend {
    /// A plaintext, unencrypted file backend at `path`.
    pub fn plain(path: impl Into<PathBuf>) -> Self {
        FileStateBackend {
            path: path.into(),
            passphrase: None,
        }
    }

    /// An encrypted file backend at `path`, decrypting/encrypting with
    /// `passphrase` (e.g. read from an environment variable).
    pub fn encrypted(path: impl Into<PathBuf>, passphrase: impl Into<String>) -> Self {
        FileStateBackend {
            path: path.into(),
            passphrase: Some(passphrase.into()),
        }
    }

    /// The lock file path (`<state>.lock`).
    fn lock_path(&self) -> PathBuf {
        let base = self
            .path
            .file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_else(|| "state".into());
        let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
        parent.join(format!("{base}.lock"))
    }

    /// The history file path (`<state>.history`).
    fn history_path(&self) -> PathBuf {
        let base = self
            .path
            .file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_else(|| "state".into());
        let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
        parent.join(format!("{base}.history"))
    }

    /// Read the raw envelope JSON text (or `None` if the file is absent).
    fn read_envelope_text(&self) -> Result<Option<String>, StateError> {
        if !self.path.exists() {
            return Ok(None);
        }
        std::fs::read_to_string(&self.path)
            .map(Some)
            .map_err(|e| StateError::Io(e.to_string()))
    }

    /// Parse an envelope into a [`State`], applying decryption when needed.
    fn parse_envelope(&self, text: &str) -> Result<State, StateError> {
        let envelope: Envelope =
            serde_json::from_str(text).map_err(|e| StateError::Envelope(e.to_string()))?;
        let json = match envelope {
            Envelope::Plain { data } => data,
            Envelope::Encrypted {
                kdf,
                salt,
                nonce,
                ciphertext,
            } => {
                let passphrase = self
                    .passphrase
                    .as_ref()
                    .ok_or(StateError::MissingPassphrase)?;
                if kdf != KDF_TAG {
                    return Err(StateError::Envelope(format!("unknown kdf `{kdf}`")));
                }
                let salt = crate::crypto::from_hex(&salt)?;
                let nonce = crate::crypto::from_hex(&nonce)?;
                let ct = crate::crypto::from_hex(&ciphertext)?;
                let key = DerivedKey::from_passphrase_and_salt(passphrase, &salt)?;
                let plaintext = key.decrypt(&nonce, &ct).map_err(|_| StateError::Decrypt)?;
                String::from_utf8(plaintext)
                    .map_err(|_| StateError::Envelope("non-utf8 state".into()))?
            }
        };
        let state: State =
            serde_json::from_str(&json).map_err(|e| StateError::Encode(e.to_string()))?;
        if state.version > STATE_FORMAT_VERSION {
            return Err(StateError::IncompatibleVersion {
                found: state.version,
                max: STATE_FORMAT_VERSION,
            });
        }
        Ok(state)
    }

    /// Serialize a state to an envelope (encrypting if configured).
    fn build_envelope(&self, state: &State) -> Result<String, StateError> {
        let json = state.canonical_json()?;
        let envelope = match &self.passphrase {
            None => Envelope::Plain { data: json },
            Some(passphrase) => {
                let key = DerivedKey::from_passphrase(passphrase)?;
                let (nonce, ct) = key.encrypt(json.as_bytes())?;
                Envelope::Encrypted {
                    kdf: KDF_TAG.into(),
                    salt: crate::crypto::to_hex(key.salt()),
                    nonce: crate::crypto::to_hex(&nonce),
                    ciphertext: crate::crypto::to_hex(&ct),
                }
            }
        };
        serde_json::to_string_pretty(&envelope).map_err(|e| StateError::Encode(e.to_string()))
    }
}

impl StateBackend for FileStateBackend {
    fn load_state(&self) -> Result<State, StateError> {
        match self.read_envelope_text()? {
            None => Ok(State::new()),
            Some(text) => self.parse_envelope(&text),
        }
    }

    fn save_state(&self, state: &State) -> Result<(), StateError> {
        // Capture the prior state so history can compute an accurate diff.
        let prev = self
            .read_envelope_text()?
            .map(|t| self.parse_envelope(&t))
            .transpose()?;

        // Write the state envelope.
        let envelope = self.build_envelope(state)?;
        std::fs::write(&self.path, envelope).map_err(|e| StateError::Io(e.to_string()))?;

        // Append a history entry.
        let mut history = self.read_history()?;
        history.record(state, prev.as_ref())?;
        let hp = self.history_path();
        std::fs::write(&hp, history.to_json()?).map_err(|e| StateError::Io(e.to_string()))?;

        Ok(())
    }

    fn lock(&self, holder: &str, ttl: Duration) -> Result<LockGuard, StateError> {
        acquire_lock_file(&self.lock_path(), holder, ttl)
    }

    fn unlock(&self, guard: &LockGuard) -> Result<(), StateError> {
        release_lock_file(&self.lock_path(), guard)
    }

    fn read_history(&self) -> Result<StateHistory, StateError> {
        let hp = self.history_path();
        if !hp.exists() {
            return Ok(StateHistory::new());
        }
        let text = std::fs::read_to_string(&hp).map_err(|e| StateError::Io(e.to_string()))?;
        StateHistory::from_json(&text)
    }
}
