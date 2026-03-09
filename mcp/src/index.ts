#!/usr/bin/env node
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { z } from "zod";
import { MemlayerClient, type LargeResponseRef } from "./api-client.js";
import { FileCache } from "./file-cache.js";

const client = new MemlayerClient(
  process.env.MEMLAYER_SERVER_URL || "http://localhost:8420/api",
  process.env.MEMLAYER_AUTH_TOKEN || "",
);

const fileCache = new FileCache();

const server = new McpServer({
  name: "claude-mem-mcp",
  version: "0.4.0",
});

function formatLargeResponseNotice(ref: LargeResponseRef): string {
  return [
    `\n\n---\n**Large response offloaded to file** (${ref.size_bytes} bytes, type: ${ref.content_type})`,
    `**File ID:** ${ref.file_id}`,
    `\n**Summary:**\n${ref.summary}`,
    `\n**Structural Index:**\n${ref.index}`,
    `\nUse \`read_memory_file\` with file_id="${ref.file_id}" and line ranges from the index above to read specific sections.`,
  ].join("\n");
}

server.tool(
  "search_memory",
  "Search across all past Claude Code conversations using hybrid semantic + full-text search. Returns relevant conversation excerpts ranked by relevance.",
  {
    query: z
      .string()
      .describe(
        "Natural language search query describing what you are looking for",
      ),
    session_id: z
      .string()
      .uuid()
      .optional()
      .describe("Filter to a specific session ID"),
    project_path: z
      .string()
      .optional()
      .describe("Filter to conversations about a specific project path"),
    limit: z
      .number()
      .min(1)
      .max(50)
      .default(10)
      .describe("Maximum number of results to return"),
    after: z
      .string()
      .datetime()
      .optional()
      .describe(
        "Filter to entries after this ISO 8601 timestamp (e.g., '2025-01-01T00:00:00Z')",
      ),
    before: z
      .string()
      .datetime()
      .optional()
      .describe("Filter to entries before this ISO 8601 timestamp"),
    types: z
      .array(
        z.enum(["user", "assistant", "tool_use", "tool_result"]),
      )
      .optional()
      .describe("Filter by message content types"),
  },
  async ({ query, session_id, project_path, limit, after, before, types }) => {
    try {
      const results = await client.search({
        query,
        session_id,
        project_path,
        limit,
        after,
        before,
        types,
      });

      if (results.results.length === 0) {
        return {
          content: [
            { type: "text" as const, text: "No matching memories found." },
          ],
        };
      }

      const formatted = results.results
        .map((r, i) => {
          const header = `### Result ${i + 1} (score: ${r.rrf_score.toFixed(3)})`;
          const meta = `**Session:** ${r.session_id} | **Project:** ${r.project_path || "unknown"} | **Date:** ${r.created_at} | **Type:** ${r.content_type}${r.tool_name ? ` (${r.tool_name})` : ""}`;
          const content =
            r.raw_content.length > 50000
              ? r.raw_content.substring(0, 50000) + "...[truncated]"
              : r.raw_content;
          return `${header}\n${meta}\n\n${content}`;
        })
        .join("\n\n---\n\n");

      let text = `Found ${results.total} results (showing top ${results.results.length}, search: ${results.search_ms.toFixed(0)}ms):\n\n${formatted}`;

      // Handle large response offloading
      if (results.large_response) {
        const ref = results.large_response;
        await fileCache.ensureCached(ref.file_id, () =>
          client.downloadFile(ref.file_id),
        );
        text += formatLargeResponseNotice(ref);
      }

      return {
        content: [{ type: "text" as const, text }],
      };
    } catch (e) {
      return {
        content: [
          {
            type: "text" as const,
            text: `Memory search error: ${e instanceof Error ? e.message : String(e)}`,
          },
        ],
        isError: true,
      };
    }
  },
);

server.tool(
  "get_session_summary",
  "Retrieve the full chronological conversation history for a specific Claude Code session. Use after search_memory returns interesting results to get full context.",
  {
    session_id: z.string().uuid().describe("The session ID to retrieve"),
    limit: z
      .number()
      .min(1)
      .max(500)
      .default(200)
      .describe("Max entries to return"),
    types: z
      .array(
        z.enum(["user", "assistant", "tool_use", "tool_result"]),
      )
      .optional()
      .describe("Filter by message content types"),
  },
  async ({ session_id, limit, types }) => {
    try {
      const summary = await client.getSessionSummary(session_id, limit, types);

      if (!summary || summary.messages.length === 0) {
        return {
          content: [
            {
              type: "text" as const,
              text: `No data found for session ${session_id}.`,
            },
          ],
        };
      }

      const header = `## Session: ${summary.session_id}\n**Project:** ${summary.project_path || "unknown"}\n**Slug:** ${summary.slug || "none"}\n**Started:** ${summary.created_at}\n**Messages:** ${summary.message_count}`;

      const messages = summary.messages
        .map((m) => {
          const role = m.message_type === "user" ? "Human" : "Assistant";
          const typeTag =
            m.content_type !== "text" ? ` [${m.content_type}]` : "";
          const toolTag = m.tool_name ? ` (${m.tool_name})` : "";
          const content =
            m.raw_content.length > 50000
              ? m.raw_content.substring(0, 50000) + "...[truncated]"
              : m.raw_content;
          return `**[${role}${typeTag}${toolTag}]** (${m.created_at})\n${content}`;
        })
        .join("\n\n");

      let text = `${header}\n\n${messages}`;

      // Handle large response offloading
      if (summary.large_response) {
        const ref = summary.large_response;
        await fileCache.ensureCached(ref.file_id, () =>
          client.downloadFile(ref.file_id),
        );
        text += formatLargeResponseNotice(ref);
      }

      return {
        content: [{ type: "text" as const, text }],
      };
    } catch (e) {
      return {
        content: [
          {
            type: "text" as const,
            text: `Session summary error: ${e instanceof Error ? e.message : String(e)}`,
          },
        ],
        isError: true,
      };
    }
  },
);

server.tool(
  "read_memory_file",
  "Read a specific line range from a large response file that was offloaded during search or session summary. Use the structural index from the previous search/summary result to identify which lines to read.",
  {
    file_id: z
      .string()
      .uuid()
      .describe("The file ID from the large_response reference"),
    start_line: z
      .number()
      .min(1)
      .describe("Start line number (1-indexed, inclusive)"),
    end_line: z
      .number()
      .min(1)
      .describe("End line number (1-indexed, inclusive)"),
  },
  async ({ file_id, start_line, end_line }) => {
    try {
      const localPath = await fileCache.ensureCached(file_id, () =>
        client.downloadFile(file_id),
      );

      const content = fileCache.readLines(localPath, start_line, end_line);

      return {
        content: [
          {
            type: "text" as const,
            text: `Lines ${start_line}-${end_line} of file ${file_id}:\n\n${content}`,
          },
        ],
      };
    } catch (e) {
      return {
        content: [
          {
            type: "text" as const,
            text: `File read error: ${e instanceof Error ? e.message : String(e)}`,
          },
        ],
        isError: true,
      };
    }
  },
);

const transport = new StdioServerTransport();
await server.connect(transport);
