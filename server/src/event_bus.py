"""In-process pub/sub for broadcasting events to SSE clients."""

import asyncio
import logging
from collections import defaultdict

logger = logging.getLogger(__name__)


class EventBus:
    def __init__(self):
        self._subscribers: dict[int, asyncio.Queue] = {}
        self._counter = 0

    def subscribe(self, maxsize: int = 256) -> tuple[int, asyncio.Queue]:
        """Create a new subscriber. Returns (id, queue)."""
        self._counter += 1
        queue: asyncio.Queue = asyncio.Queue(maxsize=maxsize)
        self._subscribers[self._counter] = queue
        logger.debug(
            "SSE subscriber added (id=%d, total=%d)",
            self._counter,
            len(self._subscribers),
        )
        return self._counter, queue

    def unsubscribe(self, subscriber_id: int):
        """Remove a subscriber."""
        self._subscribers.pop(subscriber_id, None)
        logger.debug(
            "SSE subscriber removed (id=%d, total=%d)",
            subscriber_id,
            len(self._subscribers),
        )

    def publish(self, event: dict):
        """Non-blocking publish to all subscribers. Drops on full queues."""
        dead = []
        for sub_id, queue in self._subscribers.items():
            try:
                queue.put_nowait(event)
            except asyncio.QueueFull:
                dead.append(sub_id)
                logger.warning("SSE subscriber %d queue full, dropping", sub_id)
        for sub_id in dead:
            self._subscribers.pop(sub_id, None)

    @property
    def subscriber_count(self) -> int:
        return len(self._subscribers)


# Module-level singleton
event_bus = EventBus()
