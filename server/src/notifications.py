"""
Email notification for incompatible client connections.

Rate-limited to at most one email per hour. Tracks incompatible client
connections for the /api/admin/incompatible-clients endpoint.
"""

import logging
import smtplib
import time
from collections import defaultdict
from dataclasses import dataclass, field
from email.message import EmailMessage
from threading import Lock

from .config import settings

logger = logging.getLogger(__name__)


@dataclass
class IncompatibleClientRecord:
    client_version: str
    component: str
    first_seen: float
    last_seen: float
    count: int = 1


class IncompatibleClientTracker:
    """Tracks and notifies about incompatible client connections."""

    def __init__(self):
        self._records: dict[str, IncompatibleClientRecord] = {}
        self._lock = Lock()
        self._last_email_sent: float = 0
        self._min_interval = 3600  # 1 hour between emails

    def record(self, client_version: str, component: str = "unknown"):
        """Record an incompatible client connection attempt."""
        key = f"{client_version}:{component}"
        now = time.time()

        with self._lock:
            if key in self._records:
                self._records[key].last_seen = now
                self._records[key].count += 1
            else:
                self._records[key] = IncompatibleClientRecord(
                    client_version=client_version,
                    component=component,
                    first_seen=now,
                    last_seen=now,
                )

        self._maybe_send_email()

    def get_records(self) -> list[dict]:
        """Return all incompatible client records for the admin endpoint."""
        with self._lock:
            return [
                {
                    "client_version": r.client_version,
                    "component": r.component,
                    "first_seen": r.first_seen,
                    "last_seen": r.last_seen,
                    "count": r.count,
                }
                for r in self._records.values()
            ]

    def _maybe_send_email(self):
        """Send notification email if configured and rate limit allows."""
        email = getattr(settings, "notification_email", "")
        if not email:
            return

        now = time.time()
        if now - self._last_email_sent < self._min_interval:
            return

        smtp_host = getattr(settings, "notification_smtp_host", "")
        if not smtp_host:
            return

        with self._lock:
            if not self._records:
                return
            self._last_email_sent = now
            records = list(self._records.values())

        try:
            self._send_email(email, records)
        except Exception as e:
            logger.error(f"Failed to send incompatible client notification: {e}")

    def _send_email(self, to: str, records: list[IncompatibleClientRecord]):
        from .version import SERVER_VERSION

        body_lines = [
            f"Memlayer server (v{SERVER_VERSION}) has received connections "
            "from incompatible clients:\n",
        ]
        for r in records:
            body_lines.append(
                f"  - {r.component} v{r.client_version}: "
                f"{r.count} attempt(s), last seen {time.ctime(r.last_seen)}"
            )
        body_lines.append(
            "\nClients should be updated to a compatible version. "
            "Run 'memlayer update' on each client machine."
        )

        msg = EmailMessage()
        msg["Subject"] = f"[memlayer] Incompatible client connections detected"
        msg["From"] = getattr(settings, "notification_smtp_user", f"memlayer@{getattr(settings, 'notification_smtp_host', 'localhost')}")
        msg["To"] = to
        msg.set_content("\n".join(body_lines))

        smtp_host = getattr(settings, "notification_smtp_host", "")
        smtp_port = getattr(settings, "notification_smtp_port", 587)
        smtp_user = getattr(settings, "notification_smtp_user", "")
        smtp_pass = getattr(settings, "notification_smtp_password", "")

        with smtplib.SMTP(smtp_host, smtp_port, timeout=30) as server:
            server.ehlo()
            if smtp_port != 25:
                server.starttls()
            if smtp_user:
                server.login(smtp_user, smtp_pass)
            server.send_message(msg)

        logger.info(f"Sent incompatible client notification to {to}")


# Global tracker instance
tracker = IncompatibleClientTracker()
