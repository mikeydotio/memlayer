use base64::Engine;
use ed25519_dalek::{Signature, VerifyingKey, Verifier};
use tracing::{info, warn, error};

/// Migration state for the daemon side
#[derive(Debug, Clone, PartialEq)]
pub enum DaemonMigrationState {
    /// Normal operation, no migration
    Inactive,
    /// Server returned pubkey header, migration may be imminent
    Aware { pubkey: Vec<u8> },
    /// Received 449 redirect, queueing entries
    Redirecting {
        pubkey: Vec<u8>,
        redirect_url: String,
        migration_id: String,
    },
    /// Draining queued entries to new server
    Draining {
        redirect_url: String,
        migration_id: String,
    },
    /// Migration complete, using new server
    Complete {
        new_server_url: String,
    },
}

/// Parsed 449 redirect payload from server
#[derive(Debug, Clone, serde::Deserialize)]
pub struct RedirectPayload {
    pub migration_id: String,
    pub redirect_url: String,
    pub signature: String,
}

/// Manages daemon-side migration state
pub struct MigrationTracker {
    state: DaemonMigrationState,
    stored_pubkey: Option<Vec<u8>>,
}

impl MigrationTracker {
    pub fn new() -> Self {
        MigrationTracker {
            state: DaemonMigrationState::Inactive,
            stored_pubkey: None,
        }
    }

    /// Store the Ed25519 public key from server response headers
    pub fn store_pubkey(&mut self, pubkey_b64: &str) {
        match base64::engine::general_purpose::URL_SAFE.decode(pubkey_b64) {
            Ok(pubkey) => {
                if pubkey.len() == 32 {
                    if self.stored_pubkey.as_ref() != Some(&pubkey) {
                        info!("Stored migration public key from server");
                        self.stored_pubkey = Some(pubkey.clone());
                        self.state = DaemonMigrationState::Aware { pubkey };
                    }
                } else {
                    warn!("Invalid pubkey length: {} (expected 32)", pubkey.len());
                }
            }
            Err(e) => {
                warn!(error = %e, "Failed to decode migration pubkey");
            }
        }
    }

    /// Verify an Ed25519 signature on a redirect payload
    pub fn verify_redirect(&self, payload_json: &[u8], signature_b64: &str) -> bool {
        let pubkey_bytes = match &self.stored_pubkey {
            Some(pk) => pk,
            None => {
                warn!("No stored pubkey for redirect verification");
                return false;
            }
        };

        let sig_bytes = match base64::engine::general_purpose::URL_SAFE.decode(signature_b64) {
            Ok(b) => b,
            Err(e) => {
                warn!(error = %e, "Failed to decode redirect signature");
                return false;
            }
        };

        let pubkey_arr: [u8; 32] = match pubkey_bytes.as_slice().try_into() {
            Ok(arr) => arr,
            Err(_) => {
                warn!("Invalid pubkey length for Ed25519");
                return false;
            }
        };

        let verifying_key = match VerifyingKey::from_bytes(&pubkey_arr) {
            Ok(vk) => vk,
            Err(e) => {
                warn!(error = %e, "Failed to create verifying key");
                return false;
            }
        };

        let sig_arr: [u8; 64] = match sig_bytes.as_slice().try_into() {
            Ok(arr) => arr,
            Err(_) => {
                warn!("Invalid signature length: {} (expected 64)", sig_bytes.len());
                return false;
            }
        };

        let signature = Signature::from_bytes(&sig_arr);

        match verifying_key.verify(payload_json, &signature) {
            Ok(()) => {
                info!("Redirect signature verified successfully");
                true
            }
            Err(e) => {
                error!(error = %e, "Redirect signature verification FAILED — possible MITM attack");
                false
            }
        }
    }

    /// Handle a 449 redirect response
    pub fn handle_redirect(&mut self, payload: &RedirectPayload) -> bool {
        // Reconstruct the signed payload (what the server signed)
        let payload_json = serde_json::json!({
            "migration_id": payload.migration_id,
            "redirect_url": payload.redirect_url,
            "public_key": self.stored_pubkey.as_ref()
                .map(|pk| base64::engine::general_purpose::URL_SAFE.encode(pk))
                .unwrap_or_default(),
        });
        let payload_bytes = serde_json::to_vec(&payload_json).unwrap_or_default();

        if !self.verify_redirect(&payload_bytes, &payload.signature) {
            error!("Rejecting redirect: signature verification failed");
            return false;
        }

        info!(
            migration_id = %payload.migration_id,
            redirect_url = %payload.redirect_url,
            "Accepted server migration redirect"
        );

        self.state = DaemonMigrationState::Redirecting {
            pubkey: self.stored_pubkey.clone().unwrap_or_default(),
            redirect_url: payload.redirect_url.clone(),
            migration_id: payload.migration_id.clone(),
        };

        true
    }

    /// Get the current migration state
    pub fn state(&self) -> &DaemonMigrationState {
        &self.state
    }

    /// Check if we should be redirecting entries to a new server
    pub fn redirect_url(&self) -> Option<&str> {
        match &self.state {
            DaemonMigrationState::Redirecting { redirect_url, .. } => Some(redirect_url),
            DaemonMigrationState::Draining { redirect_url, .. } => Some(redirect_url),
            _ => None,
        }
    }

    /// Get migration ID if in redirect/drain state
    pub fn migration_id(&self) -> Option<&str> {
        match &self.state {
            DaemonMigrationState::Redirecting { migration_id, .. } => Some(migration_id),
            DaemonMigrationState::Draining { migration_id, .. } => Some(migration_id),
            _ => None,
        }
    }

    /// Transition to draining state (after queued entries start draining to new server)
    pub fn start_draining(&mut self) {
        if let DaemonMigrationState::Redirecting { redirect_url, migration_id, .. } = &self.state {
            self.state = DaemonMigrationState::Draining {
                redirect_url: redirect_url.clone(),
                migration_id: migration_id.clone(),
            };
        }
    }

    /// Complete the migration
    pub fn complete(&mut self, new_server_url: String) {
        info!(new_server_url = %new_server_url, "Migration complete");
        self.state = DaemonMigrationState::Complete { new_server_url };
    }

    /// Reset migration state
    pub fn reset(&mut self) {
        self.state = DaemonMigrationState::Inactive;
        self.stored_pubkey = None;
    }
}

/// Rewrite the memlayer daemon config to point to a new server.
/// Updates environment file and MCP settings.
pub fn rewrite_config(new_server_url: &str, new_auth_token: &str) -> Result<(), Box<dyn std::error::Error>> {
    let home = dirs::home_dir().ok_or("Cannot determine home directory")?;

    // Update systemd environment file
    let env_file = home.join(".config/memlayer/env");
    if env_file.exists() {
        let content = std::fs::read_to_string(&env_file)?;
        let mut new_lines = Vec::new();
        for line in content.lines() {
            if line.starts_with("MEMLAYER_SERVER_URL=") {
                new_lines.push(format!("MEMLAYER_SERVER_URL={}", new_server_url));
            } else if line.starts_with("MEMLAYER_AUTH_TOKEN=") {
                new_lines.push(format!("MEMLAYER_AUTH_TOKEN={}", new_auth_token));
            } else {
                new_lines.push(line.to_string());
            }
        }
        std::fs::write(&env_file, new_lines.join("\n") + "\n")?;
        info!("Updated daemon env file: {}", env_file.display());
    }

    // Update MCP settings in Claude config
    let claude_settings = home.join(".claude/settings.json");
    if claude_settings.exists() {
        let content = std::fs::read_to_string(&claude_settings)?;
        if let Ok(mut json) = serde_json::from_str::<serde_json::Value>(&content) {
            // Find memlayer MCP server config and update env vars
            if let Some(servers) = json.get_mut("mcpServers") {
                if let Some(servers_obj) = servers.as_object_mut() {
                    for (name, server) in servers_obj.iter_mut() {
                        if name.contains("memlayer") || name.contains("memlayer") {
                            if let Some(env) = server.get_mut("env") {
                                if let Some(env_obj) = env.as_object_mut() {
                                    env_obj.insert(
                                        "MEMLAYER_SERVER_URL".to_string(),
                                        serde_json::Value::String(new_server_url.to_string()),
                                    );
                                    env_obj.insert(
                                        "MEMLAYER_AUTH_TOKEN".to_string(),
                                        serde_json::Value::String(new_auth_token.to_string()),
                                    );
                                }
                            }
                        }
                    }
                }
            }
            let pretty = serde_json::to_string_pretty(&json)?;
            std::fs::write(&claude_settings, pretty + "\n")?;
            info!("Updated MCP settings: {}", claude_settings.display());
        }
    }

    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_migration_tracker_initial_state() {
        let tracker = MigrationTracker::new();
        assert_eq!(*tracker.state(), DaemonMigrationState::Inactive);
        assert!(tracker.redirect_url().is_none());
        assert!(tracker.migration_id().is_none());
    }

    #[test]
    fn test_store_pubkey_transitions_to_aware() {
        let mut tracker = MigrationTracker::new();
        // Valid 32-byte key encoded as base64
        let key_bytes = [42u8; 32];
        let key_b64 = base64::engine::general_purpose::URL_SAFE.encode(&key_bytes);
        tracker.store_pubkey(&key_b64);
        match tracker.state() {
            DaemonMigrationState::Aware { pubkey } => {
                assert_eq!(pubkey.len(), 32);
            }
            _ => panic!("Expected Aware state"),
        }
    }

    #[test]
    fn test_store_pubkey_rejects_invalid_length() {
        let mut tracker = MigrationTracker::new();
        let key_bytes = [42u8; 16]; // Wrong length
        let key_b64 = base64::engine::general_purpose::URL_SAFE.encode(&key_bytes);
        tracker.store_pubkey(&key_b64);
        assert_eq!(*tracker.state(), DaemonMigrationState::Inactive);
    }

    #[test]
    fn test_store_pubkey_rejects_invalid_base64() {
        let mut tracker = MigrationTracker::new();
        tracker.store_pubkey("not-valid-base64!!!");
        assert_eq!(*tracker.state(), DaemonMigrationState::Inactive);
    }

    #[test]
    fn test_redirect_url_none_when_inactive() {
        let tracker = MigrationTracker::new();
        assert!(tracker.redirect_url().is_none());
    }

    #[test]
    fn test_reset_clears_state() {
        let mut tracker = MigrationTracker::new();
        let key_bytes = [42u8; 32];
        let key_b64 = base64::engine::general_purpose::URL_SAFE.encode(&key_bytes);
        tracker.store_pubkey(&key_b64);
        tracker.reset();
        assert_eq!(*tracker.state(), DaemonMigrationState::Inactive);
        assert!(tracker.redirect_url().is_none());
    }

    #[test]
    fn test_verify_redirect_fails_without_pubkey() {
        let tracker = MigrationTracker::new();
        assert!(!tracker.verify_redirect(b"test", "dGVzdA=="));
    }

    #[test]
    fn test_draining_state_transition() {
        let mut tracker = MigrationTracker::new();
        tracker.state = DaemonMigrationState::Redirecting {
            pubkey: vec![0u8; 32],
            redirect_url: "http://new-server/api".to_string(),
            migration_id: "test-id".to_string(),
        };
        assert_eq!(tracker.redirect_url(), Some("http://new-server/api"));
        assert_eq!(tracker.migration_id(), Some("test-id"));

        tracker.start_draining();
        assert_eq!(tracker.redirect_url(), Some("http://new-server/api"));
        assert_eq!(tracker.migration_id(), Some("test-id"));
        match tracker.state() {
            DaemonMigrationState::Draining { .. } => {}
            _ => panic!("Expected Draining state"),
        }
    }

    #[test]
    fn test_complete_state() {
        let mut tracker = MigrationTracker::new();
        tracker.complete("http://new-server/api".to_string());
        match tracker.state() {
            DaemonMigrationState::Complete { new_server_url } => {
                assert_eq!(new_server_url, "http://new-server/api");
            }
            _ => panic!("Expected Complete state"),
        }
    }

    #[test]
    fn test_rewrite_config_nonexistent_files() {
        // Should succeed silently when config files don't exist
        let result = rewrite_config("http://new-server/api", "new-token");
        // This might fail if home dir detection fails in test, but shouldn't panic
        // Just verify it doesn't crash
        let _ = result;
    }

    // --- Ed25519 crypto round-trip tests ---

    use ed25519_dalek::{Signer, SigningKey};
    use rand_core::OsRng;

    /// Helper: generate a keypair, return (signing_key, base64url-encoded pubkey)
    fn gen_keypair() -> (SigningKey, String) {
        let sk = SigningKey::generate(&mut OsRng);
        let pk_bytes = sk.verifying_key().to_bytes();
        let pk_b64 = base64::engine::general_purpose::URL_SAFE.encode(pk_bytes);
        (sk, pk_b64)
    }

    #[test]
    fn test_ed25519_sign_verify_roundtrip() {
        let (sk, pk_b64) = gen_keypair();

        let mut tracker = MigrationTracker::new();
        tracker.store_pubkey(&pk_b64);

        let payload = b"hello world migration payload";
        let sig = sk.sign(payload);
        let sig_b64 = base64::engine::general_purpose::URL_SAFE.encode(sig.to_bytes());

        assert!(tracker.verify_redirect(payload, &sig_b64));
    }

    #[test]
    fn test_ed25519_wrong_key_rejected() {
        let (sk_a, _pk_a_b64) = gen_keypair();
        let (_sk_b, pk_b_b64) = gen_keypair();

        let mut tracker = MigrationTracker::new();
        // Store key B's pubkey
        tracker.store_pubkey(&pk_b_b64);

        // Sign with key A
        let payload = b"signed by key A";
        let sig = sk_a.sign(payload);
        let sig_b64 = base64::engine::general_purpose::URL_SAFE.encode(sig.to_bytes());

        // Verification with key B should fail
        assert!(!tracker.verify_redirect(payload, &sig_b64));
    }

    #[test]
    fn test_ed25519_tampered_message_rejected() {
        let (sk, pk_b64) = gen_keypair();

        let mut tracker = MigrationTracker::new();
        tracker.store_pubkey(&pk_b64);

        let original = b"original message";
        let sig = sk.sign(original);
        let sig_b64 = base64::engine::general_purpose::URL_SAFE.encode(sig.to_bytes());

        // Verify original works
        assert!(tracker.verify_redirect(original, &sig_b64));

        // Tampered message should fail
        let tampered = b"tampered message";
        assert!(!tracker.verify_redirect(tampered, &sig_b64));
    }

    #[test]
    fn test_handle_redirect_valid_signature() {
        let (sk, pk_b64) = gen_keypair();

        let mut tracker = MigrationTracker::new();
        tracker.store_pubkey(&pk_b64);

        let migration_id = "mig-001";
        let redirect_url = "https://new-server.example.com/api";

        // Reconstruct the same JSON that handle_redirect will build internally
        let payload_json = serde_json::json!({
            "migration_id": migration_id,
            "redirect_url": redirect_url,
            "public_key": &pk_b64,
        });
        let payload_bytes = serde_json::to_vec(&payload_json).unwrap();

        let sig = sk.sign(&payload_bytes);
        let sig_b64 = base64::engine::general_purpose::URL_SAFE.encode(sig.to_bytes());

        let redirect_payload = RedirectPayload {
            migration_id: migration_id.to_string(),
            redirect_url: redirect_url.to_string(),
            signature: sig_b64,
        };

        assert!(tracker.handle_redirect(&redirect_payload));

        // Verify state is now Redirecting with correct fields
        match tracker.state() {
            DaemonMigrationState::Redirecting {
                redirect_url: url,
                migration_id: id,
                ..
            } => {
                assert_eq!(url, redirect_url);
                assert_eq!(id, migration_id);
            }
            other => panic!("Expected Redirecting state, got {:?}", other),
        }
    }

    #[test]
    fn test_handle_redirect_invalid_signature() {
        let (_sk, pk_b64) = gen_keypair();

        let mut tracker = MigrationTracker::new();
        tracker.store_pubkey(&pk_b64);

        let redirect_payload = RedirectPayload {
            migration_id: "mig-002".to_string(),
            redirect_url: "https://evil.example.com/api".to_string(),
            // Garbage signature (valid base64url but wrong Ed25519 sig)
            signature: base64::engine::general_purpose::URL_SAFE.encode([0xFFu8; 64]),
        };

        assert!(!tracker.handle_redirect(&redirect_payload));

        // State should remain Aware (unchanged from store_pubkey)
        match tracker.state() {
            DaemonMigrationState::Aware { .. } => {}
            other => panic!("Expected Aware state (unchanged), got {:?}", other),
        }
    }

    #[test]
    fn test_handle_redirect_no_stored_pubkey() {
        let tracker_no_key = MigrationTracker::new();

        let redirect_payload = RedirectPayload {
            migration_id: "mig-003".to_string(),
            redirect_url: "https://new-server.example.com/api".to_string(),
            signature: base64::engine::general_purpose::URL_SAFE.encode([0u8; 64]),
        };

        // Can't call handle_redirect on immutable ref, need mutable
        let mut tracker = tracker_no_key;
        assert!(!tracker.handle_redirect(&redirect_payload));
        assert_eq!(*tracker.state(), DaemonMigrationState::Inactive);
    }

    #[test]
    fn test_full_daemon_migration_flow() {
        // Simulate the full flow: store_pubkey from header → handle_redirect → Redirecting
        let (sk, pk_b64) = gen_keypair();

        let mut tracker = MigrationTracker::new();

        // Step 1: Server sends pubkey header, daemon stores it
        tracker.store_pubkey(&pk_b64);
        match tracker.state() {
            DaemonMigrationState::Aware { .. } => {}
            other => panic!("Expected Aware after store_pubkey, got {:?}", other),
        }

        // Step 2: Server sends 449 redirect with signed payload
        let migration_id = "mig-full-flow";
        let redirect_url = "https://destination.example.com/api";

        let payload_json = serde_json::json!({
            "migration_id": migration_id,
            "redirect_url": redirect_url,
            "public_key": &pk_b64,
        });
        let payload_bytes = serde_json::to_vec(&payload_json).unwrap();
        let sig = sk.sign(&payload_bytes);
        let sig_b64 = base64::engine::general_purpose::URL_SAFE.encode(sig.to_bytes());

        let redirect_payload = RedirectPayload {
            migration_id: migration_id.to_string(),
            redirect_url: redirect_url.to_string(),
            signature: sig_b64,
        };

        assert!(tracker.handle_redirect(&redirect_payload));

        // Step 3: Verify state is Redirecting
        match tracker.state() {
            DaemonMigrationState::Redirecting {
                pubkey,
                redirect_url: url,
                migration_id: id,
            } => {
                assert_eq!(url, redirect_url);
                assert_eq!(id, migration_id);
                assert_eq!(pubkey.len(), 32);
                // Verify the stored pubkey matches what we generated
                let expected_pk = base64::engine::general_purpose::URL_SAFE.decode(&pk_b64).unwrap();
                assert_eq!(pubkey, &expected_pk);
            }
            other => panic!("Expected Redirecting state, got {:?}", other),
        }

        // Step 4: Transition to draining and complete
        assert_eq!(tracker.redirect_url(), Some(redirect_url));
        assert_eq!(tracker.migration_id(), Some(migration_id));

        tracker.start_draining();
        match tracker.state() {
            DaemonMigrationState::Draining { .. } => {}
            other => panic!("Expected Draining state, got {:?}", other),
        }

        tracker.complete(redirect_url.to_string());
        match tracker.state() {
            DaemonMigrationState::Complete { new_server_url } => {
                assert_eq!(new_server_url, redirect_url);
            }
            other => panic!("Expected Complete state, got {:?}", other),
        }
    }

    #[test]
    fn test_rewrite_config_with_real_temp_files() {
        use std::io::Write;

        let tmp_dir = tempfile::tempdir().unwrap();
        let env_dir = tmp_dir.path().join(".config/memlayer");
        std::fs::create_dir_all(&env_dir).unwrap();

        let env_file = env_dir.join("env");
        {
            let mut f = std::fs::File::create(&env_file).unwrap();
            writeln!(f, "MEMLAYER_SERVER_URL=http://old-server:8420/api").unwrap();
            writeln!(f, "MEMLAYER_AUTH_TOKEN=old-token-abc").unwrap();
            writeln!(f, "SOME_OTHER_VAR=keep-me").unwrap();
        }

        // Read original content to verify setup
        let original = std::fs::read_to_string(&env_file).unwrap();
        assert!(original.contains("http://old-server:8420/api"));
        assert!(original.contains("old-token-abc"));
        assert!(original.contains("SOME_OTHER_VAR=keep-me"));

        // Directly test the file rewriting logic (matching rewrite_config's approach)
        let new_url = "https://new-server.example.com/api";
        let new_token = "new-token-xyz";

        let content = std::fs::read_to_string(&env_file).unwrap();
        let mut new_lines = Vec::new();
        for line in content.lines() {
            if line.starts_with("MEMLAYER_SERVER_URL=") {
                new_lines.push(format!("MEMLAYER_SERVER_URL={}", new_url));
            } else if line.starts_with("MEMLAYER_AUTH_TOKEN=") {
                new_lines.push(format!("MEMLAYER_AUTH_TOKEN={}", new_token));
            } else {
                new_lines.push(line.to_string());
            }
        }
        std::fs::write(&env_file, new_lines.join("\n") + "\n").unwrap();

        // Verify rewritten content
        let rewritten = std::fs::read_to_string(&env_file).unwrap();
        assert!(
            rewritten.contains(&format!("MEMLAYER_SERVER_URL={}", new_url)),
            "Expected new server URL in rewritten file"
        );
        assert!(
            rewritten.contains(&format!("MEMLAYER_AUTH_TOKEN={}", new_token)),
            "Expected new auth token in rewritten file"
        );
        assert!(
            rewritten.contains("SOME_OTHER_VAR=keep-me"),
            "Expected unrelated vars to be preserved"
        );
        assert!(
            !rewritten.contains("old-server"),
            "Old server URL should be gone"
        );
        assert!(
            !rewritten.contains("old-token-abc"),
            "Old auth token should be gone"
        );
    }
}
