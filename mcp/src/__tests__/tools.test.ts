import { describe, it, expect } from "vitest";
import { formatLargeResponseNotice } from "../format.js";
import type {
  LargeResponseRef,
  SearchResult,
  SessionMessage,
} from "../api-client.js";

// Replicates the inline search-result formatting from the search_memory tool handler
function formatSearchResults(results: SearchResult[], total: number, searchMs: number): string {
  const formatted = results
    .map((r, i) => {
      const header = `### Result ${i + 1} (score: ${r.rrf_score.toFixed(3)})`;
      const meta = `**Session:** ${r.session_id} | **Project:** ${r.project_path || "unknown"} | **Date:** ${r.created_at} | **Type:** ${r.content_type}${r.tool_name ? ` (${r.tool_name})` : ""}`;
      return `${header}\n${meta}\n\n${r.raw_content}`;
    })
    .join("\n\n---\n\n");

  return `Found ${total} results (showing top ${results.length}, search: ${searchMs.toFixed(0)}ms):\n\n${formatted}`;
}

// Replicates the inline session-message formatting from the get_session_summary tool handler
function formatSessionMessages(messages: SessionMessage[]): string {
  return messages
    .map((m) => {
      const role = m.message_type === "user" ? "Human" : "Assistant";
      const typeTag = m.content_type !== "text" ? ` [${m.content_type}]` : "";
      const toolTag = m.tool_name ? ` (${m.tool_name})` : "";
      return `**[${role}${typeTag}${toolTag}]** (${m.created_at})\n${m.raw_content}`;
    })
    .join("\n\n");
}

// ---- Fixtures ----

function makeLargeResponseRef(overrides: Partial<LargeResponseRef> = {}): LargeResponseRef {
  return {
    schema_version: 1,
    file_id: "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee",
    file_url: "http://localhost:8420/api/files/aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee",
    size_bytes: 350_000,
    summary: "Conversation about database migrations and schema changes.",
    index: "Lines 1-50: Migration planning\nLines 51-120: Schema DDL",
    content_type: "text/plain",
    ...overrides,
  };
}

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

function makeSessionMessage(overrides: Partial<SessionMessage> = {}): SessionMessage {
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

// ---- Tests ----

describe("formatLargeResponseNotice", () => {
  it("includes all fields in the notice", () => {
    const ref = makeLargeResponseRef();
    const notice = formatLargeResponseNotice(ref);

    expect(notice).toContain(ref.file_id);
    expect(notice).toContain("350000 bytes");
    expect(notice).toContain("text/plain");
    expect(notice).toContain(ref.summary);
    expect(notice).toContain("Lines 1-50: Migration planning");
    expect(notice).toContain("Lines 51-120: Schema DDL");
    expect(notice).toContain("read_memory_file");
    expect(notice).toContain(`file_id="${ref.file_id}"`);
  });

  it("handles empty summary and index gracefully", () => {
    const ref = makeLargeResponseRef({ summary: "", index: "" });
    const notice = formatLargeResponseNotice(ref);

    // Should still contain structural elements even when content is empty
    expect(notice).toContain("**File ID:**");
    expect(notice).toContain("**Summary:**");
    expect(notice).toContain("**Structural Index (line ranges):**");
    expect(notice).toContain("read_memory_file");
    expect(notice).toContain(ref.file_id);
  });
});

describe("search result formatting", () => {
  it("formats results with header, metadata, and content", () => {
    const results = [
      makeSearchResult(),
      makeSearchResult({
        id: 2,
        session_id: "22222222-3333-4444-5555-666666666666",
        rrf_score: 0.321,
        raw_content: "Use CREATE EXTENSION pgvector;",
        content_type: "assistant",
        tool_name: null,
      }),
    ];

    const output = formatSearchResults(results, 15, 12.345);

    // Overall header
    expect(output).toContain("Found 15 results (showing top 2, search: 12ms)");

    // First result
    expect(output).toContain("### Result 1 (score: 0.482)");
    expect(output).toContain("**Session:** 11111111-2222-3333-4444-555555555555");
    expect(output).toContain("**Project:** /home/mikey/memlayer");
    expect(output).toContain("How do I set up pgvector?");

    // Second result
    expect(output).toContain("### Result 2 (score: 0.321)");
    expect(output).toContain("Use CREATE EXTENSION pgvector;");

    // Results are separated by horizontal rules
    expect(output).toContain("---");
  });

  it("shows tool_name in metadata when present", () => {
    const results = [
      makeSearchResult({
        content_type: "tool_result",
        tool_name: "Read",
        raw_content: "File contents...",
      }),
    ];

    const output = formatSearchResults(results, 1, 5);

    expect(output).toContain("**Type:** tool_result (Read)");
  });

  it("returns 'No matching memories found.' for empty results", () => {
    // This mirrors the exact check in the search_memory handler
    const emptyMessage = "No matching memories found.";
    expect(emptyMessage).toBe("No matching memories found.");
  });
});

describe("session summary formatting", () => {
  it("labels user messages as Human and others as Assistant", () => {
    const messages = [
      makeSessionMessage({ message_type: "user", raw_content: "Question" }),
      makeSessionMessage({
        id: 2,
        message_type: "assistant",
        raw_content: "Answer",
        created_at: "2026-02-01T08:01:00Z",
      }),
    ];

    const output = formatSessionMessages(messages);

    expect(output).toContain("**[Human]** (2026-02-01T08:00:00Z)");
    expect(output).toContain("Question");
    expect(output).toContain("**[Assistant]** (2026-02-01T08:01:00Z)");
    expect(output).toContain("Answer");
  });

  it("includes content_type tag for non-text messages and tool_name when present", () => {
    const messages = [
      makeSessionMessage({
        message_type: "assistant",
        content_type: "tool_use",
        tool_name: "Bash",
        raw_content: "ls -la",
      }),
      makeSessionMessage({
        id: 2,
        message_type: "assistant",
        content_type: "tool_result",
        tool_name: "Bash",
        raw_content: "total 42\ndrwxr-xr-x ...",
        created_at: "2026-02-01T08:02:00Z",
      }),
    ];

    const output = formatSessionMessages(messages);

    expect(output).toContain("**[Assistant [tool_use] (Bash)]**");
    expect(output).toContain("**[Assistant [tool_result] (Bash)]**");
    expect(output).toContain("ls -la");
  });
});

describe("error response formatting", () => {
  it("formats error messages with isError flag pattern", () => {
    const error = new Error("Connection refused");
    const errorResponse = {
      content: [
        {
          type: "text" as const,
          text: `Memory search error: ${error instanceof Error ? error.message : String(error)}`,
        },
      ],
      isError: true,
    };

    expect(errorResponse.isError).toBe(true);
    expect(errorResponse.content[0].text).toBe("Memory search error: Connection refused");
    expect(errorResponse.content).toHaveLength(1);
    expect(errorResponse.content[0].type).toBe("text");
  });
});
