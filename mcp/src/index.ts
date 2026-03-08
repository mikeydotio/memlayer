#!/usr/bin/env node
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { z } from "zod";
import { MemlayerClient } from "./api-client.js";

const client = new MemlayerClient(
  process.env.MEMLAYER_SERVER_URL || "http://localhost:8420/api",
  process.env.MEMLAYER_AUTH_TOKEN || "",
);

const server = new McpServer({
  name: "claude-mem-mcp",
  version: "0.1.0",
});

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
  },
  async ({ query, session_id, project_path, limit }) => {
    try {
      const results = await client.search({
        query,
        session_id,
        project_path,
        limit,
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
            r.raw_content.length > 1500
              ? r.raw_content.substring(0, 1500) + "...[truncated]"
              : r.raw_content;
          return `${header}\n${meta}\n\n${content}`;
        })
        .join("\n\n---\n\n");

      return {
        content: [
          {
            type: "text" as const,
            text: `Found ${results.total} results (showing top ${results.results.length}, search: ${results.search_ms.toFixed(0)}ms):\n\n${formatted}`,
          },
        ],
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
  },
  async ({ session_id, limit }) => {
    try {
      const summary = await client.getSessionSummary(session_id, limit);

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
            m.raw_content.length > 2000
              ? m.raw_content.substring(0, 2000) + "...[truncated]"
              : m.raw_content;
          return `**[${role}${typeTag}${toolTag}]** (${m.created_at})\n${content}`;
        })
        .join("\n\n");

      return {
        content: [{ type: "text" as const, text: `${header}\n\n${messages}` }],
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

const transport = new StdioServerTransport();
await server.connect(transport);
