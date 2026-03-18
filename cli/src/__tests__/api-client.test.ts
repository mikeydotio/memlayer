import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { MemlayerClient } from "../api-client.js";

let client: MemlayerClient;

beforeEach(() => {
  client = new MemlayerClient("http://localhost:8420/api", "test-token");
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("MemlayerClient", () => {
  describe("search", () => {
    it("sends correct request and parses response", async () => {
      const mockResponse = {
        results: [
          {
            id: 1,
            session_id: "sess-1",
            message_type: "user",
            content_type: "user",
            raw_content: "hello",
            tool_name: null,
            created_at: "2026-01-01T00:00:00Z",
            project_path: "/project",
            fts_rank: 1,
            vector_rank: 2,
            rrf_score: 0.5,
          },
        ],
        total: 1,
        query_embedding_ms: 10,
        search_ms: 5,
      };

      vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
        new Response(JSON.stringify(mockResponse), { status: 200 }),
      );

      const result = await client.search({ query: "hello", limit: 10 });
      expect(result.total).toBe(1);
      expect(result.results[0].raw_content).toBe("hello");

      const call = vi.mocked(fetch).mock.calls[0];
      expect(call[0]).toBe("http://localhost:8420/api/search");
      const init = call[1] as RequestInit;
      expect(init.method).toBe("POST");
      expect(JSON.parse(init.body as string)).toEqual({
        query: "hello",
        limit: 10,
      });
    });

    it("includes auth header", async () => {
      vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
        new Response(JSON.stringify({ results: [], total: 0, query_embedding_ms: 0, search_ms: 0 }), { status: 200 }),
      );

      await client.search({ query: "test", limit: 5 });

      const call = vi.mocked(fetch).mock.calls[0];
      const headers = (call[1] as RequestInit).headers as Record<string, string>;
      expect(headers["Authorization"]).toBe("Bearer test-token");
    });

    it("throws on non-200 response", async () => {
      vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
        new Response("Unauthorized", { status: 401 }),
      );

      await expect(client.search({ query: "test", limit: 5 })).rejects.toThrow(
        "Search failed: 401",
      );
    });
  });

  describe("getSessionSummary", () => {
    it("sends correct request with types filter", async () => {
      const mockResponse = {
        session_id: "sess-1",
        project_path: "/project",
        slug: "test",
        created_at: "2026-01-01T00:00:00Z",
        message_count: 0,
        messages: [],
      };

      vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
        new Response(JSON.stringify(mockResponse), { status: 200 }),
      );

      await client.getSessionSummary("sess-1", 100, ["user", "assistant"]);

      const call = vi.mocked(fetch).mock.calls[0];
      expect(call[0]).toBe(
        "http://localhost:8420/api/sessions/sess-1/summary?limit=100&types=user,assistant",
      );
    });

    it("throws on non-200 response", async () => {
      vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
        new Response("Not Found", { status: 404 }),
      );

      await expect(client.getSessionSummary("bad-id")).rejects.toThrow(
        "Session summary failed: 404",
      );
    });
  });

  describe("downloadFile", () => {
    it("returns file content as text", async () => {
      vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
        new Response("file content here", { status: 200 }),
      );

      const content = await client.downloadFile("file-123");
      expect(content).toBe("file content here");
    });

    it("throws on non-200 response", async () => {
      vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
        new Response("Not Found", { status: 404 }),
      );

      await expect(client.downloadFile("bad-id")).rejects.toThrow(
        "File download failed: 404",
      );
    });
  });
});
