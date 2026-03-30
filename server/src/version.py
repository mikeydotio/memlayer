"""
Version compatibility and feature negotiation for memlayer.

Provides:
- Version parsing and comparison
- Compatibility checking (major mismatch, min version enforcement)
- Feature registry mapping features to the minor version that introduced them
- Read-only mode state
"""

import logging
import re
from enum import Enum

logger = logging.getLogger(__name__)

try:
    from ._version import __version__ as SERVER_VERSION
except ImportError:
    SERVER_VERSION = "0.0.0-dev"


class CompatResult(str, Enum):
    OK = "ok"
    MINOR_MISMATCH = "minor_mismatch"
    MAJOR_MISMATCH = "major_mismatch"
    UPGRADE_REQUIRED = "upgrade_required"


def parse_version(s: str) -> tuple[int, int, int]:
    """Parse a version string like '2.1.0' or 'v2.1.0' into (major, minor, patch)."""
    s = s.strip().lstrip("v")
    match = re.match(r"(\d+)\.(\d+)\.(\d+)", s)
    if not match:
        return (0, 0, 0)
    return (int(match.group(1)), int(match.group(2)), int(match.group(3)))


def version_string(v: tuple[int, int, int]) -> str:
    return f"{v[0]}.{v[1]}.{v[2]}"


def check_compatibility(
    client_version: str,
    server_version: str | None = None,
    min_client_version: str | None = None,
) -> CompatResult:
    """Check whether a client version is compatible with the server.

    Returns:
        CompatResult.OK - fully compatible
        CompatResult.MINOR_MISMATCH - same major, different minor (allowed with feature flags)
        CompatResult.MAJOR_MISMATCH - different major version (reject)
        CompatResult.UPGRADE_REQUIRED - below minimum required version (reject)
    """
    sv = server_version or SERVER_VERSION
    client = parse_version(client_version)
    server = parse_version(sv)

    # Check minimum required version first (critical updates)
    if min_client_version:
        min_ver = parse_version(min_client_version)
        if client < min_ver:
            return CompatResult.UPGRADE_REQUIRED

    # Major version mismatch
    if client[0] != server[0]:
        return CompatResult.MAJOR_MISMATCH

    # Minor version difference (informational, not a rejection)
    if client[1] != server[1]:
        return CompatResult.MINOR_MISMATCH

    return CompatResult.OK


# Feature registry: maps (major, minor) to features introduced in that version.
# Features are cumulative — a v2.3 client has all features from v2.0, v2.1, v2.2, v2.3.
FEATURE_REGISTRY: dict[tuple[int, int], list[str]] = {
    (1, 0): ["search", "ingest", "session_summary"],
    (1, 4): ["offline_queue", "batch_ingest"],
    (1, 5): ["migration_api", "embedding_metadata"],
    (1, 6): ["browse_api", "session_entries", "stream_sse"],
    (1, 7): ["recent_sessions", "all_types_filter", "full_content"],
    (2, 0): ["knowledge_graph", "graph_search", "file_storage", "extraction"],
}


def features_for_version(version: str) -> list[str]:
    """Return the cumulative feature list available to a given client version."""
    ver = parse_version(version)
    features = []
    for (major, minor), feats in sorted(FEATURE_REGISTRY.items()):
        if (major, minor) <= (ver[0], ver[1]):
            features.extend(feats)
    return features


# Read-only mode state (set at startup by main.py)
read_only: bool = False
