"""Unit tests for migration state machine, key generation, and Ed25519 crypto.

Tests pure logic only — no database connection required.
"""

import base64
import hashlib
import os

import pytest
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from cryptography.hazmat.primitives.serialization import (
    Encoding,
    NoEncryption,
    PrivateFormat,
    PublicFormat,
)

from src.migration_state import (
    MigrationRole,
    MigrationState,
    VALID_TRANSITIONS,
)


class TestMigrationEnums:
    """Tests for MigrationRole and MigrationState enum values."""

    def test_migration_role_values(self):
        assert MigrationRole.SOURCE == "source"
        assert MigrationRole.DESTINATION == "destination"

    def test_migration_role_count(self):
        """There should be exactly two roles."""
        assert len(MigrationRole) == 2

    def test_migration_role_is_str_enum(self):
        """MigrationRole members should be usable as plain strings."""
        assert isinstance(MigrationRole.SOURCE, str)
        assert MigrationRole.SOURCE == "source"
        assert MigrationRole.SOURCE.value == "source"

    def test_migration_state_values(self):
        assert MigrationState.IDLE == "IDLE"
        assert MigrationState.INITIATED == "INITIATED"
        assert MigrationState.KEY_EXCHANGED == "KEY_EXCHANGED"
        assert MigrationState.REDIRECTING == "REDIRECTING"
        assert MigrationState.DRAINING == "DRAINING"
        assert MigrationState.TRANSFERRING == "TRANSFERRING"
        assert MigrationState.VERIFYING == "VERIFYING"
        assert MigrationState.COMPLETE == "COMPLETE"
        assert MigrationState.FAILED == "FAILED"

    def test_migration_state_count(self):
        """There should be exactly 9 states."""
        assert len(MigrationState) == 9

    def test_migration_state_is_str_enum(self):
        """MigrationState members should be usable as plain strings."""
        assert isinstance(MigrationState.IDLE, str)
        assert MigrationState.IDLE == "IDLE"
        assert MigrationState.IDLE.value == "IDLE"

    def test_migration_state_from_string(self):
        """States should be constructable from their string values."""
        assert MigrationState("IDLE") == MigrationState.IDLE
        assert MigrationState("COMPLETE") == MigrationState.COMPLETE
        assert MigrationState("FAILED") == MigrationState.FAILED

    def test_migration_state_invalid_string(self):
        """Constructing a state from an invalid string should raise ValueError."""
        with pytest.raises(ValueError):
            MigrationState("NONEXISTENT")

    def test_all_states_in_transitions(self):
        """Every state must appear as a key in VALID_TRANSITIONS."""
        for state in MigrationState:
            assert state in VALID_TRANSITIONS, f"{state} missing from VALID_TRANSITIONS"


class TestValidTransitions:
    """Tests for the VALID_TRANSITIONS map completeness and correctness."""

    def test_idle_can_initiate(self):
        assert MigrationState.INITIATED in VALID_TRANSITIONS[MigrationState.IDLE]

    def test_idle_cannot_skip_to_transferring(self):
        assert MigrationState.TRANSFERRING not in VALID_TRANSITIONS[MigrationState.IDLE]

    def test_idle_cannot_skip_to_key_exchanged(self):
        assert MigrationState.KEY_EXCHANGED not in VALID_TRANSITIONS[MigrationState.IDLE]

    def test_idle_cannot_skip_to_redirecting(self):
        assert MigrationState.REDIRECTING not in VALID_TRANSITIONS[MigrationState.IDLE]

    def test_idle_cannot_skip_to_complete(self):
        assert MigrationState.COMPLETE not in VALID_TRANSITIONS[MigrationState.IDLE]

    def test_idle_has_single_forward_transition(self):
        """IDLE should only transition to INITIATED (no FAILED, no self-loop)."""
        assert VALID_TRANSITIONS[MigrationState.IDLE] == {MigrationState.INITIATED}

    def test_all_states_can_fail(self):
        """All non-terminal states (except IDLE) should be able to transition to FAILED."""
        non_terminal = [
            s
            for s in MigrationState
            if s not in (MigrationState.IDLE, MigrationState.COMPLETE, MigrationState.FAILED)
        ]
        for state in non_terminal:
            assert MigrationState.FAILED in VALID_TRANSITIONS[state], (
                f"{state} can't transition to FAILED"
            )

    def test_all_active_states_can_cancel(self):
        """All active (non-IDLE, non-terminal) states should be able to return to IDLE."""
        active = [
            s
            for s in MigrationState
            if s not in (MigrationState.IDLE, MigrationState.COMPLETE, MigrationState.FAILED)
        ]
        for state in active:
            assert MigrationState.IDLE in VALID_TRANSITIONS[state], (
                f"{state} can't cancel (return to IDLE)"
            )

    def test_forward_path(self):
        """Test the happy path: IDLE -> INITIATED -> ... -> COMPLETE."""
        path = [
            MigrationState.IDLE,
            MigrationState.INITIATED,
            MigrationState.KEY_EXCHANGED,
            MigrationState.REDIRECTING,
            MigrationState.DRAINING,
            MigrationState.TRANSFERRING,
            MigrationState.VERIFYING,
            MigrationState.COMPLETE,
        ]
        for i in range(len(path) - 1):
            assert path[i + 1] in VALID_TRANSITIONS[path[i]], (
                f"Can't transition from {path[i]} to {path[i + 1]}"
            )

    def test_complete_can_return_to_idle(self):
        assert MigrationState.IDLE in VALID_TRANSITIONS[MigrationState.COMPLETE]

    def test_complete_only_goes_to_idle(self):
        """COMPLETE is terminal — only transition is back to IDLE."""
        assert VALID_TRANSITIONS[MigrationState.COMPLETE] == {MigrationState.IDLE}

    def test_failed_can_return_to_idle(self):
        assert MigrationState.IDLE in VALID_TRANSITIONS[MigrationState.FAILED]

    def test_failed_only_goes_to_idle(self):
        """FAILED is terminal — only transition is back to IDLE."""
        assert VALID_TRANSITIONS[MigrationState.FAILED] == {MigrationState.IDLE}

    def test_no_self_loops(self):
        """No state should transition to itself."""
        for state, targets in VALID_TRANSITIONS.items():
            assert state not in targets, f"{state} has a self-loop"

    def test_no_orphan_target_states(self):
        """Every state mentioned as a transition target must also be a key in the map."""
        all_targets = set()
        for targets in VALID_TRANSITIONS.values():
            all_targets.update(targets)
        for target in all_targets:
            assert target in VALID_TRANSITIONS, (
                f"Target state {target} is not a key in VALID_TRANSITIONS"
            )

    def test_transitions_map_completeness(self):
        """VALID_TRANSITIONS should have exactly as many keys as there are states."""
        assert len(VALID_TRANSITIONS) == len(MigrationState)

    def test_each_active_state_has_expected_target_count(self):
        """Each active state (not IDLE/COMPLETE/FAILED) should have at least one forward
        neighbor plus FAILED and IDLE (3+ targets).
        KEY_EXCHANGED has 4 targets because it can go to REDIRECTING (source path)
        or TRANSFERRING (destination path)."""
        active = [
            s
            for s in MigrationState
            if s not in (MigrationState.IDLE, MigrationState.COMPLETE, MigrationState.FAILED)
        ]
        for state in active:
            targets = VALID_TRANSITIONS[state]
            if state == MigrationState.KEY_EXCHANGED:
                assert len(targets) == 4, (
                    f"{state} has {len(targets)} targets, expected 4 "
                    "(REDIRECTING + TRANSFERRING + FAILED + IDLE)"
                )
            else:
                assert len(targets) == 3, (
                    f"{state} has {len(targets)} targets, expected 3 (forward + FAILED + IDLE)"
                )

    @pytest.mark.parametrize(
        "from_state, to_state",
        [
            (MigrationState.IDLE, MigrationState.INITIATED),
            (MigrationState.INITIATED, MigrationState.KEY_EXCHANGED),
            (MigrationState.KEY_EXCHANGED, MigrationState.REDIRECTING),
            (MigrationState.REDIRECTING, MigrationState.DRAINING),
            (MigrationState.DRAINING, MigrationState.TRANSFERRING),
            (MigrationState.TRANSFERRING, MigrationState.VERIFYING),
            (MigrationState.VERIFYING, MigrationState.COMPLETE),
            (MigrationState.COMPLETE, MigrationState.IDLE),
            (MigrationState.FAILED, MigrationState.IDLE),
        ],
    )
    def test_valid_transition_parametrized(self, from_state, to_state):
        """Each explicitly expected transition should be valid."""
        assert to_state in VALID_TRANSITIONS[from_state]

    @pytest.mark.parametrize(
        "from_state, to_state",
        [
            (MigrationState.IDLE, MigrationState.COMPLETE),
            (MigrationState.IDLE, MigrationState.FAILED),
            (MigrationState.IDLE, MigrationState.TRANSFERRING),
            (MigrationState.INITIATED, MigrationState.COMPLETE),
            (MigrationState.INITIATED, MigrationState.TRANSFERRING),
            (MigrationState.KEY_EXCHANGED, MigrationState.COMPLETE),
            (MigrationState.REDIRECTING, MigrationState.COMPLETE),
            (MigrationState.REDIRECTING, MigrationState.KEY_EXCHANGED),
            (MigrationState.DRAINING, MigrationState.REDIRECTING),
            (MigrationState.TRANSFERRING, MigrationState.DRAINING),
            (MigrationState.VERIFYING, MigrationState.TRANSFERRING),
            (MigrationState.COMPLETE, MigrationState.INITIATED),
            (MigrationState.COMPLETE, MigrationState.FAILED),
            (MigrationState.FAILED, MigrationState.INITIATED),
            (MigrationState.FAILED, MigrationState.COMPLETE),
        ],
    )
    def test_invalid_transition_parametrized(self, from_state, to_state):
        """Each explicitly forbidden transition should be invalid."""
        assert to_state not in VALID_TRANSITIONS[from_state]


class TestMigrationKey:
    """Tests for migration key generation format and hashing."""

    def test_key_generation_format(self):
        """Migration key should be base64url-encoded 32 random bytes."""
        raw = os.urandom(32)
        key = base64.urlsafe_b64encode(raw).decode()
        assert len(key) == 44  # 32 bytes -> 44 chars in base64url (with padding)
        decoded = base64.urlsafe_b64decode(key)
        assert len(decoded) == 32
        assert decoded == raw

    def test_key_is_url_safe(self):
        """Key should use only URL-safe characters (no + or /)."""
        for _ in range(20):
            raw = os.urandom(32)
            key = base64.urlsafe_b64encode(raw).decode()
            assert "+" not in key
            assert "/" not in key

    def test_key_hash_is_sha256(self):
        raw = os.urandom(32)
        key = base64.urlsafe_b64encode(raw).decode()
        key_hash = hashlib.sha256(key.encode()).hexdigest()
        assert len(key_hash) == 64
        # Hash should be deterministic
        assert key_hash == hashlib.sha256(key.encode()).hexdigest()

    def test_key_hash_is_lowercase_hex(self):
        raw = os.urandom(32)
        key = base64.urlsafe_b64encode(raw).decode()
        key_hash = hashlib.sha256(key.encode()).hexdigest()
        assert key_hash == key_hash.lower()
        assert all(c in "0123456789abcdef" for c in key_hash)

    def test_different_keys_different_hashes(self):
        key1 = base64.urlsafe_b64encode(os.urandom(32)).decode()
        key2 = base64.urlsafe_b64encode(os.urandom(32)).decode()
        hash1 = hashlib.sha256(key1.encode()).hexdigest()
        hash2 = hashlib.sha256(key2.encode()).hexdigest()
        assert hash1 != hash2

    def test_key_uniqueness(self):
        """Multiple generated keys should all be unique."""
        keys = {base64.urlsafe_b64encode(os.urandom(32)).decode() for _ in range(100)}
        assert len(keys) == 100

    def test_hash_of_key_not_reversible(self):
        """The hash should not equal the key itself (basic sanity)."""
        raw = os.urandom(32)
        key = base64.urlsafe_b64encode(raw).decode()
        key_hash = hashlib.sha256(key.encode()).hexdigest()
        assert key_hash != key


class TestEd25519Crypto:
    """Tests for Ed25519 key generation, signing, and verification."""

    def test_keypair_generation(self):
        private_key = Ed25519PrivateKey.generate()
        public_key = private_key.public_key()
        priv_bytes = private_key.private_bytes(
            Encoding.Raw, PrivateFormat.Raw, NoEncryption()
        )
        pub_bytes = public_key.public_bytes(Encoding.Raw, PublicFormat.Raw)
        assert len(priv_bytes) == 32
        assert len(pub_bytes) == 32

    def test_keypair_bytes_differ(self):
        """Private and public key bytes should not be identical."""
        private_key = Ed25519PrivateKey.generate()
        priv_bytes = private_key.private_bytes(
            Encoding.Raw, PrivateFormat.Raw, NoEncryption()
        )
        pub_bytes = private_key.public_key().public_bytes(
            Encoding.Raw, PublicFormat.Raw
        )
        assert priv_bytes != pub_bytes

    def test_different_keypairs_are_unique(self):
        """Two generated keypairs should have different key material."""
        key1 = Ed25519PrivateKey.generate()
        key2 = Ed25519PrivateKey.generate()
        pub1 = key1.public_key().public_bytes(Encoding.Raw, PublicFormat.Raw)
        pub2 = key2.public_key().public_bytes(Encoding.Raw, PublicFormat.Raw)
        assert pub1 != pub2

    def test_sign_and_verify(self):
        private_key = Ed25519PrivateKey.generate()
        public_key = private_key.public_key()
        message = b'{"migration_id": "test", "redirect_url": "http://new-server/api"}'
        signature = private_key.sign(message)
        assert len(signature) == 64
        # Should not raise
        public_key.verify(signature, message)

    def test_sign_empty_message(self):
        """Signing an empty message should work."""
        private_key = Ed25519PrivateKey.generate()
        signature = private_key.sign(b"")
        assert len(signature) == 64
        private_key.public_key().verify(signature, b"")

    def test_verify_fails_on_wrong_message(self):
        private_key = Ed25519PrivateKey.generate()
        public_key = private_key.public_key()
        message = b"correct message"
        signature = private_key.sign(message)
        with pytest.raises(Exception):
            public_key.verify(signature, b"wrong message")

    def test_verify_fails_on_tampered_message(self):
        """Even a single-byte difference should invalidate the signature."""
        private_key = Ed25519PrivateKey.generate()
        public_key = private_key.public_key()
        message = b"original message"
        signature = private_key.sign(message)
        tampered = b"Original message"  # capitalized O
        with pytest.raises(Exception):
            public_key.verify(signature, tampered)

    def test_verify_fails_on_wrong_key(self):
        key1 = Ed25519PrivateKey.generate()
        key2 = Ed25519PrivateKey.generate()
        message = b"test message"
        signature = key1.sign(message)
        with pytest.raises(Exception):
            key2.public_key().verify(signature, message)

    def test_signature_base64_roundtrip(self):
        """Signatures should survive base64url encoding/decoding."""
        private_key = Ed25519PrivateKey.generate()
        message = b"test"
        signature = private_key.sign(message)
        b64_sig = base64.urlsafe_b64encode(signature).decode()
        decoded = base64.urlsafe_b64decode(b64_sig)
        assert decoded == signature

    def test_public_key_base64_roundtrip(self):
        """Public keys should survive base64url encoding/decoding (as used in API responses)."""
        private_key = Ed25519PrivateKey.generate()
        pub_bytes = private_key.public_key().public_bytes(
            Encoding.Raw, PublicFormat.Raw
        )
        b64_pub = base64.urlsafe_b64encode(pub_bytes).decode()
        decoded = base64.urlsafe_b64decode(b64_pub)
        assert decoded == pub_bytes
        assert len(b64_pub) == 44  # 32 bytes -> 44 chars in base64url

    def test_private_key_reconstruction(self):
        """A private key should be reconstructable from its raw bytes."""
        original = Ed25519PrivateKey.generate()
        raw_bytes = original.private_bytes(
            Encoding.Raw, PrivateFormat.Raw, NoEncryption()
        )
        reconstructed = Ed25519PrivateKey.from_private_bytes(raw_bytes)
        # Sign with original, verify with reconstructed's public key
        message = b"roundtrip test"
        signature = original.sign(message)
        reconstructed.public_key().verify(signature, message)

    def test_sign_redirect_payload_format(self):
        """Simulate what MigrationManager.sign_redirect does."""
        private_key = Ed25519PrivateKey.generate()
        private_key_bytes = private_key.private_bytes(
            Encoding.Raw, PrivateFormat.Raw, NoEncryption()
        )
        message = b'{"migration_id": "abc-123", "redirect_url": "http://new.example.com/api"}'

        # Reconstruct key from bytes (as sign_redirect does)
        restored_key = Ed25519PrivateKey.from_private_bytes(private_key_bytes)
        signature = restored_key.sign(message)
        b64_signature = base64.urlsafe_b64encode(signature).decode()

        # Verify using original public key
        private_key.public_key().verify(
            base64.urlsafe_b64decode(b64_signature), message
        )


class TestStateMachineTransitionValidation:
    """Tests that simulate the _transition validation logic from MigrationManager."""

    def _is_valid_transition(self, from_state: MigrationState, to_state: MigrationState) -> bool:
        """Reproduce the validation logic from MigrationManager._transition."""
        return to_state in VALID_TRANSITIONS.get(from_state, set())

    def test_all_forward_transitions_valid(self):
        """Walking the entire happy path should pass validation at every step."""
        path = [
            MigrationState.IDLE,
            MigrationState.INITIATED,
            MigrationState.KEY_EXCHANGED,
            MigrationState.REDIRECTING,
            MigrationState.DRAINING,
            MigrationState.TRANSFERRING,
            MigrationState.VERIFYING,
            MigrationState.COMPLETE,
        ]
        for i in range(len(path) - 1):
            assert self._is_valid_transition(path[i], path[i + 1])

    def test_backward_transitions_rejected(self):
        """Skipping backward (except cancel-to-IDLE) should be rejected."""
        ordered = [
            MigrationState.INITIATED,
            MigrationState.KEY_EXCHANGED,
            MigrationState.REDIRECTING,
            MigrationState.DRAINING,
            MigrationState.TRANSFERRING,
            MigrationState.VERIFYING,
            MigrationState.COMPLETE,
        ]
        for i in range(len(ordered)):
            for j in range(i):
                # Going backwards to ordered[j] should be invalid
                # unless ordered[j] == IDLE (but IDLE isn't in this list)
                assert not self._is_valid_transition(ordered[i], ordered[j]), (
                    f"Backward {ordered[i]} -> {ordered[j]} should be rejected"
                )

    def test_skip_forward_transitions_rejected(self):
        """Skipping more than one step forward should be rejected,
        except KEY_EXCHANGED -> TRANSFERRING which is valid for
        the destination-side flow (skipping REDIRECTING/DRAINING)."""
        ordered = [
            MigrationState.IDLE,
            MigrationState.INITIATED,
            MigrationState.KEY_EXCHANGED,
            MigrationState.REDIRECTING,
            MigrationState.DRAINING,
            MigrationState.TRANSFERRING,
            MigrationState.VERIFYING,
            MigrationState.COMPLETE,
        ]
        # KEY_EXCHANGED -> TRANSFERRING is allowed for destination-side transfer
        allowed_skips = {
            (MigrationState.KEY_EXCHANGED, MigrationState.TRANSFERRING),
        }
        for i in range(len(ordered)):
            for j in range(i + 2, len(ordered)):
                if (ordered[i], ordered[j]) in allowed_skips:
                    assert self._is_valid_transition(ordered[i], ordered[j]), (
                        f"Skip-forward {ordered[i]} -> {ordered[j]} should be allowed "
                        "(destination-side transfer)"
                    )
                else:
                    assert not self._is_valid_transition(ordered[i], ordered[j]), (
                        f"Skip-forward {ordered[i]} -> {ordered[j]} should be rejected"
                    )

    def test_cancel_from_all_active_states(self):
        """Cancel (transition to IDLE) should work from every active state."""
        active = [
            MigrationState.INITIATED,
            MigrationState.KEY_EXCHANGED,
            MigrationState.REDIRECTING,
            MigrationState.DRAINING,
            MigrationState.TRANSFERRING,
            MigrationState.VERIFYING,
        ]
        for state in active:
            assert self._is_valid_transition(state, MigrationState.IDLE)

    def test_fail_from_all_active_states(self):
        """Failure should be possible from every active state."""
        active = [
            MigrationState.INITIATED,
            MigrationState.KEY_EXCHANGED,
            MigrationState.REDIRECTING,
            MigrationState.DRAINING,
            MigrationState.TRANSFERRING,
            MigrationState.VERIFYING,
        ]
        for state in active:
            assert self._is_valid_transition(state, MigrationState.FAILED)

    def test_cannot_fail_from_idle(self):
        assert not self._is_valid_transition(MigrationState.IDLE, MigrationState.FAILED)

    def test_cannot_fail_from_complete(self):
        assert not self._is_valid_transition(MigrationState.COMPLETE, MigrationState.FAILED)

    def test_cannot_fail_from_failed(self):
        assert not self._is_valid_transition(MigrationState.FAILED, MigrationState.FAILED)

    def test_recovery_from_failed(self):
        """FAILED -> IDLE should be valid (to allow retry)."""
        assert self._is_valid_transition(MigrationState.FAILED, MigrationState.IDLE)

    def test_reset_from_complete(self):
        """COMPLETE -> IDLE should be valid (to allow a new migration)."""
        assert self._is_valid_transition(MigrationState.COMPLETE, MigrationState.IDLE)

    def test_invalid_from_state_returns_false(self):
        """An unknown from_state should return False (empty set fallback)."""
        # Simulate with a direct VALID_TRANSITIONS.get call using a non-existent key
        assert "NOT_A_STATE" not in VALID_TRANSITIONS
        result = MigrationState.IDLE in VALID_TRANSITIONS.get("NOT_A_STATE", set())
        assert result is False
