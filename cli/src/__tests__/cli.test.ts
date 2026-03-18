import { describe, it, expect } from "vitest";
import {
  formatSearchJSON,
  formatSearchText,
  formatSessionJSON,
  formatSessionText,
  formatReadFileJSON,
  formatReadFileText,
  formatStatusJSON,
  formatError,
} from "../cli-formatters.js";
import type {
  SearchResponse,
  SearchResult,
  SessionSummary,
  SessionMessage,
  LargeResponseRef,
} from "../api-client.js";

// ── Fixtures ─────────────────────────────────────────────────────────

function makeSearchResult(overrides: Partial<SearchResult> = {}): SearchResult {
  return {
    id: 1,
    session_id: "11111111-2222-3333-4444-555555555555",
    message_type: "user",
    content_type: "user",
    raw_content: "How do I set up pgvector?",
    tool_name: null,
    created_at: "2026-01-15T10:30:00Z",
    project_path: "/home/mikey/memlayer",
    fts_rank: 1,
    vector_rank: 3,
    rrf_score: 0.482,
    ...overrides,
  };
}

function makeSearchResponse(
  overrides: Partial<SearchResponse> = {},
): SearchResponse {
  return {
    results: [makeSearchResult()],
    total: 1,
    query_embedding_ms: 10,
    search_ms: 12.345,
    ...overrides,
  };
}

function makeLargeResponseRef(
  overrides: Partial<LargeResponseRef> = {},
): LargeResponseRef {
  return {
    schema_version: 1,
    file_id: "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee",
    file_url:
      "http://localhost:8420/api/files/aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee",
    size_bytes: 350_000,
    summary: "Conversation about database migrations and schema changes.",
    index: "Lines 1-50: Migration planning\nLines 51-120: Schema DDL",
    content_type: "text/plain",
    ...overrides,
  };
}

function makeSessionMessage(
  overrides: Partial<SessionMessage> = {},
): SessionMessage {
  return {
    id: 1,
    message_type: "user",
    content_type: "text",
    raw_content: "Tell me about hybrid search.",
    tool_name: null,
    created_at: "2026-02-01T08:00:00Z",
    ...overrides,
  };
}

function makeSessionSummary(
  overrides: Partial<SessionSummary> = {},
): SessionSummary {
  return {
    session_id: "11111111-2222-3333-4444-555555555555",
    project_path: "/home/mikey/memlayer",
    slug: "test-session",
    created_at: "2026-02-01T08:00:00Z",
    message_count: 2,
    messages: [
      makeSessionMessage(),
      makeSessionMessage({
        id: 2,
        message_type: "assistant",
        raw_content: "Hybrid search combines FTS and vector search.",
        created_at: "2026-02-01T08:01:00Z",
      }),
    ],
    ...overrides,
  };
}

// ── Search formatter tests ───────────────────────────────────────────

describe("formatSearchJSON", () => {
  it("produces valid JSON with correct structure", () => {
    const resp = makeSearchResponse({ total: 15 });
    const output = formatSearchJSON(resp);
    const parsed = JSON.parse(output);

    expect(parsed.total).toBe(15);
    expect(parsed.count).toBe(1);
    expect(parsed.search_ms).toBe(12);
    expect(parsed.results).toHaveLength(1);
    expect(parsed.results[0].content).toBe("How do I set up pgvector?");
    expect(parsed.results[0].session_id).toBe(
      "11111111-2222-3333-4444-555555555555",
    );
    expect(parsed.large_response).toBeNull();
  });

  it("includes large_response when present", () => {
    const resp = makeSearchResponse({
      results: [],
      large_response: makeLargeResponseRef(),
    });
    const parsed = JSON.parse(formatSearchJSON(resp));

    expect(parsed.large_response).not.toBeNull();
    expect(parsed.large_response.file_id).toBe(
      "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee",
    );
    expect(parsed.large_response.size_bytes).toBe(350000);
  });
});

describe("formatSearchText", () => {
  it("formats results with headers and metadata", () => {
    const resp = makeSearchResponse({
      total: 15,
      results: [
        makeSearchResult(),
        makeSearchResult({
          id: 2,
          rrf_score: 0.321,
          raw_content: "Use CREATE EXTENSION pgvector;",
          content_type: "assistant",
        }),
      ],
    });
    const output = formatSearchText(resp);

    expect(output).toContain("Found 15 results (showing top 2, search: 12ms)");
    expect(output).toContain("### Result 1 (score: 0.482)");
    expect(output).toContain("### Result 2 (score: 0.321)");
    expect(output).toContain("How do I set up pgvector?");
    expect(output).toContain("---");
  });

  it("returns no-results message for empty results", () => {
    const resp = makeSearchResponse({ results: [], total: 0 });
    expect(formatSearchText(resp)).toBe("No matching memories found.");
  });

  it("shows large response notice with memlayer read-file hint", () => {
    const resp = makeSearchResponse({
      results: [],
      total: 150,
      large_response: makeLargeResponseRef(),
    });
    const output = formatSearchText(resp);

    expect(output).toContain("offloaded to file");
    expect(output).toContain("memlayer read-file");
    expect(output).toContain("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee");
  });

  it("shows tool_name in metadata when present", () => {
    const resp = makeSearchResponse({
      results: [
        makeSearchResult({
          content_type: "tool_result",
          tool_name: "Read",
        }),
      ],
    });
    const output = formatSearchText(resp);
    expect(output).toContain("**Type:** tool_result (Read)");
  });
});

// ── Session formatter tests ──────────────────────────────────────────

describe("formatSessionJSON", () => {
  it("serializes session summary as JSON", () => {
    const summary = makeSessionSummary();
    const parsed = JSON.parse(formatSessionJSON(summary));

    expect(parsed.session_id).toBe("11111111-2222-3333-4444-555555555555");
    expect(parsed.messages).toHaveLength(2);
  });
});

describe("formatSessionText", () => {
  it("labels user messages as Human and others as Assistant", () => {
    const summary = makeSessionSummary();
    const output = formatSessionText(summary);

    expect(output).toContain("**[Human]** (2026-02-01T08:00:00Z)");
    expect(output).toContain("**[Assistant]** (2026-02-01T08:01:00Z)");
    expect(output).toContain("## Session:");
    expect(output).toContain("**Slug:** test-session");
  });

  it("includes content_type tag for non-text messages", () => {
    const summary = makeSessionSummary({
      messages: [
        makeSessionMessage({
          message_type: "assistant",
          content_type: "tool_use",
          tool_name: "Bash",
          raw_content: "ls -la",
        }),
      ],
    });
    const output = formatSessionText(summary);
    expect(output).toContain("**[Assistant [tool_use] (Bash)]**");
  });

  it("shows no-data message for empty session", () => {
    const summary = makeSessionSummary({ messages: [], message_count: 0 });
    const output = formatSessionText(summary);
    expect(output).toContain("No data found for session");
  });

  it("shows large response notice for offloaded sessions", () => {
    const summary = makeSessionSummary({
      messages: [],
      large_response: makeLargeResponseRef(),
    });
    const output = formatSessionText(summary);
    expect(output).toContain("offloaded to file");
    expect(output).toContain("memlayer read-file");
  });
});

// ── Read-file formatter tests ────────────────────────────────────────

describe("formatReadFileJSON", () => {
  it("produces valid JSON with file info", () => {
    const parsed = JSON.parse(
      formatReadFileJSON("file-123", 10, 20, "line content"),
    );
    expect(parsed.file_id).toBe("file-123");
    expect(parsed.start_line).toBe(10);
    expect(parsed.end_line).toBe(20);
    expect(parsed.content).toBe("line content");
  });
});

describe("formatReadFileText", () => {
  it("formats with line range header", () => {
    const output = formatReadFileText("file-123", 10, 20, "line content");
    expect(output).toContain("Lines 10-20 of file file-123");
    expect(output).toContain("line content");
  });
});

// ── Error formatter tests ────────────────────────────────────────────

describe("formatError", () => {
  it("produces valid JSON error", () => {
    const parsed = JSON.parse(formatError("Connection refused"));
    expect(parsed.error).toBe("Connection refused");
  });
});

// ── Status formatter tests ───────────────────────────────────────────

describe("formatStatusJSON", () => {
  it("combines health and embeddings", () => {
    const parsed = JSON.parse(
      formatStatusJSON({ status: "ok" }, { provider: "openai" }),
    );
    expect(parsed.health.status).toBe("ok");
    expect(parsed.embeddings.provider).toBe("openai");
  });
});
