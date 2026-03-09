"""Response size analytics — tracks response sizes and offload frequency."""

import logging
import time
from dataclasses import dataclass, field

logger = logging.getLogger(__name__)


@dataclass
class EndpointStats:
    total_requests: int = 0
    total_bytes: int = 0
    inline_count: int = 0
    offloaded_count: int = 0
    min_bytes: int = 0
    max_bytes: int = 0


class ResponseAnalytics:
    """In-memory analytics for response sizes and offload frequency.

    Records per-endpoint stats and logs periodic summaries.
    Stats are designed to be consumed by a future web dashboard (v1.2.0).
    """

    def __init__(self, log_interval_secs: float = 300.0):
        self._stats: dict[str, EndpointStats] = {}
        self._log_interval = log_interval_secs
        self._last_log_time = time.monotonic()

    def _get_stats(self, endpoint: str) -> EndpointStats:
        if endpoint not in self._stats:
            self._stats[endpoint] = EndpointStats()
        return self._stats[endpoint]

    def record(self, endpoint: str, response_bytes: int, offloaded: bool) -> None:
        """Record a response for analytics."""
        stats = self._get_stats(endpoint)
        stats.total_requests += 1
        stats.total_bytes += response_bytes
        if offloaded:
            stats.offloaded_count += 1
        else:
            stats.inline_count += 1
        if stats.min_bytes == 0 or response_bytes < stats.min_bytes:
            stats.min_bytes = response_bytes
        if response_bytes > stats.max_bytes:
            stats.max_bytes = response_bytes

        logger.debug(
            "response_analytics endpoint=%s bytes=%d offloaded=%s",
            endpoint,
            response_bytes,
            offloaded,
        )

        # Periodic summary log
        now = time.monotonic()
        if now - self._last_log_time >= self._log_interval:
            self._log_summary()
            self._last_log_time = now

    def get_stats(self) -> dict[str, dict]:
        """Return stats as a plain dict (for future API exposure)."""
        return {
            endpoint: {
                "total_requests": s.total_requests,
                "total_bytes": s.total_bytes,
                "avg_bytes": s.total_bytes // s.total_requests if s.total_requests else 0,
                "min_bytes": s.min_bytes,
                "max_bytes": s.max_bytes,
                "inline_count": s.inline_count,
                "offloaded_count": s.offloaded_count,
                "offload_rate": (
                    s.offloaded_count / s.total_requests if s.total_requests else 0.0
                ),
            }
            for endpoint, s in self._stats.items()
        }

    def _log_summary(self) -> None:
        for endpoint, s in self._stats.items():
            if s.total_requests == 0:
                continue
            avg = s.total_bytes // s.total_requests
            rate = s.offloaded_count / s.total_requests * 100
            logger.info(
                "analytics_summary endpoint=%s requests=%d avg_bytes=%d "
                "min=%d max=%d offloaded=%d (%.1f%%)",
                endpoint,
                s.total_requests,
                avg,
                s.min_bytes,
                s.max_bytes,
                s.offloaded_count,
                rate,
            )


# Module-level singleton
response_analytics = ResponseAnalytics()
