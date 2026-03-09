"""Unit tests for ResponseAnalytics."""

from src.analytics import ResponseAnalytics


class TestResponseAnalytics:
    def test_record_inline_response(self):
        a = ResponseAnalytics()
        a.record("/api/search", 1500, offloaded=False)
        stats = a.get_stats()
        assert stats["/api/search"]["total_requests"] == 1
        assert stats["/api/search"]["inline_count"] == 1
        assert stats["/api/search"]["offloaded_count"] == 0
        assert stats["/api/search"]["total_bytes"] == 1500
        assert stats["/api/search"]["offload_rate"] == 0.0

    def test_record_offloaded_response(self):
        a = ResponseAnalytics()
        a.record("/api/search", 300000, offloaded=True)
        stats = a.get_stats()
        assert stats["/api/search"]["offloaded_count"] == 1
        assert stats["/api/search"]["offload_rate"] == 1.0

    def test_multiple_records(self):
        a = ResponseAnalytics()
        a.record("/api/search", 1000, offloaded=False)
        a.record("/api/search", 500000, offloaded=True)
        a.record("/api/search", 2000, offloaded=False)
        stats = a.get_stats()
        s = stats["/api/search"]
        assert s["total_requests"] == 3
        assert s["inline_count"] == 2
        assert s["offloaded_count"] == 1
        assert s["min_bytes"] == 1000
        assert s["max_bytes"] == 500000
        assert s["avg_bytes"] == (1000 + 500000 + 2000) // 3

    def test_separate_endpoints(self):
        a = ResponseAnalytics()
        a.record("/api/search", 1000, offloaded=False)
        a.record("/api/sessions/summary", 2000, offloaded=True)
        stats = a.get_stats()
        assert "/api/search" in stats
        assert "/api/sessions/summary" in stats
        assert stats["/api/search"]["total_requests"] == 1
        assert stats["/api/sessions/summary"]["total_requests"] == 1

    def test_empty_stats(self):
        a = ResponseAnalytics()
        assert a.get_stats() == {}
