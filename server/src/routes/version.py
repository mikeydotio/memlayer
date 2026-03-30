"""Version and update endpoints."""

import logging

from fastapi import APIRouter, Request

from ..version import SERVER_VERSION, features_for_version, read_only

logger = logging.getLogger(__name__)
router = APIRouter()


@router.get("/version")
async def get_version(request: Request):
    """Public version info endpoint (no auth required — exempted in auth middleware)."""
    schema_version = getattr(request.app.state, "schema_version", 0)
    is_read_only = getattr(request.app.state, "read_only", False)

    return {
        "server_version": SERVER_VERSION,
        "schema_version": schema_version,
        "min_client_version": request.app.state.min_client_version
        if hasattr(request.app.state, "min_client_version")
        else None,
        "read_only": is_read_only,
        "features": features_for_version(SERVER_VERSION),
    }


@router.get("/version/latest")
async def get_latest_version(request: Request):
    """Latest version manifest for client auto-update.

    Returns download URLs and metadata for the latest release.
    Auth required (handled by middleware).
    """
    # For now, return a static response pointing to GitHub Releases.
    # A future enhancement can cache the GitHub Releases API response.
    return {
        "latest_version": SERVER_VERSION,
        "manual_intervention": False,
        "min_client_version": getattr(request.app.state, "min_client_version", None),
        "components": {
            "daemon": {
                "version": SERVER_VERSION,
                "artifacts": {
                    "linux-x86_64": f"https://github.com/mikeydotio/memlayer/releases/download/v{SERVER_VERSION}/memlayer-daemon-linux-x86_64.tar.gz",
                    "linux-aarch64": f"https://github.com/mikeydotio/memlayer/releases/download/v{SERVER_VERSION}/memlayer-daemon-linux-aarch64.tar.gz",
                    "macos-aarch64": f"https://github.com/mikeydotio/memlayer/releases/download/v{SERVER_VERSION}/memlayer-daemon-macos-aarch64.tar.gz",
                },
            },
            "cli": {
                "version": SERVER_VERSION,
                "artifacts": {
                    "linux-x86_64": f"https://github.com/mikeydotio/memlayer/releases/download/v{SERVER_VERSION}/memlayer-cli-linux-x86_64.tar.gz",
                    "linux-aarch64": f"https://github.com/mikeydotio/memlayer/releases/download/v{SERVER_VERSION}/memlayer-cli-linux-aarch64.tar.gz",
                    "macos-aarch64": f"https://github.com/mikeydotio/memlayer/releases/download/v{SERVER_VERSION}/memlayer-cli-macos-aarch64.tar.gz",
                },
            },
        },
    }
