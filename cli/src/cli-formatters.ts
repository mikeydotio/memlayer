import type {
  SearchResult,
  SearchResponse,
  SessionSummary,
  SessionMessage,
  LargeResponseRef,
} from "./api-client.js";

// ── Search formatters ────────────────────────────────────────────────

export function formatSearchJSON(results: SearchResponse): string {
  return JSON.stringify(
    {
      total: results.total,
      count: results.results.length,
      search_ms: Math.round(results.search_ms),
      results: results.results.map((r) => ({
        session_id: r.session_id,
        project_path: r.project_path,
        created_at: r.created_at,
        content_type: r.content_type,
        tool_name: r.tool_name,
        rrf_score: r.rrf_score,
        content: r.raw_content,
      })),
      large_response: results.large_response
        ? {
            file_id: results.large_response.file_id,
            size_bytes: results.large_response.size_bytes,
            summary: results.large_response.summary,
            index: results.large_response.index,
          }
        : null,
    },
    null,
    2,
  );
}

export function formatSearchText(results: SearchResponse): string {
  if (results.large_response) {
    const ref = results.large_response;
    return [
      `Found ${results.total} results (response offloaded to file)`,
      ``,
      `**File ID:** \`${ref.file_id}\``,
      `**Size:** ${ref.size_bytes} bytes`,
      ``,
      `**Summary:**`,
      ref.summary,
      ``,
      `**Structural Index:**`,
      ref.index,
      ``,
      `Use \`memlayer read-file ${ref.file_id} --start 1 --end 50\` to read sections.`,
    ].join("\n");
  }

  if (results.results.length === 0) {
    return "No matching memories found.";
  }

  const formatted = results.results
    .map((r, i) => {
      const header = `### Result ${i + 1} (score: ${r.rrf_score.toFixed(3)})`;
      const meta = `**Session:** ${r.session_id} | **Project:** ${r.project_path || "unknown"} | **Date:** ${r.created_at} | **Type:** ${r.content_type}${r.tool_name ? ` (${r.tool_name})` : ""}`;
      return `${header}\n${meta}\n\n${r.raw_content}`;
    })
    .join("\n\n---\n\n");

  return `Found ${results.total} results (showing top ${results.results.length}, search: ${Math.round(results.search_ms)}ms):\n\n${formatted}`;
}

// ── Session formatters ───────────────────────────────────────────────

export function formatSessionJSON(summary: SessionSummary): string {
  return JSON.stringify(summary, null, 2);
}

export function formatSessionText(summary: SessionSummary): string {
  if (summary.large_response) {
    const ref = summary.large_response;
    const header = `## Session: ${summary.session_id}\n**Project:** ${summary.project_path || "unknown"}\n**Messages:** ${summary.message_count}`;
    return [
      header,
      ``,
      `Response offloaded to file (${ref.size_bytes} bytes).`,
      ``,
      `**File ID:** \`${ref.file_id}\``,
      ``,
      `**Summary:**`,
      ref.summary,
      ``,
      `**Structural Index:**`,
      ref.index,
      ``,
      `Use \`memlayer read-file ${ref.file_id} --start 1 --end 50\` to read sections.`,
    ].join("\n");
  }

  if (!summary.messages || summary.messages.length === 0) {
    return `No data found for session ${summary.session_id}.`;
  }

  const header = `## Session: ${summary.session_id}\n**Project:** ${summary.project_path || "unknown"}\n**Slug:** ${summary.slug || "none"}\n**Started:** ${summary.created_at}\n**Messages:** ${summary.message_count}`;

  const messages = summary.messages
    .map((m) => {
      const role = m.message_type === "user" ? "Human" : "Assistant";
      const typeTag = m.content_type !== "text" ? ` [${m.content_type}]` : "";
      const toolTag = m.tool_name ? ` (${m.tool_name})` : "";
      return `**[${role}${typeTag}${toolTag}]** (${m.created_at})\n${m.raw_content}`;
    })
    .join("\n\n");

  return `${header}\n\n${messages}`;
}

// ── Read-file formatters ─────────────────────────────────────────────

export function formatReadFileJSON(
  fileId: string,
  startLine: number,
  endLine: number,
  content: string,
): string {
  return JSON.stringify(
    {
      file_id: fileId,
      start_line: startLine,
      end_line: endLine,
      content,
    },
    null,
    2,
  );
}

export function formatReadFileText(
  fileId: string,
  startLine: number,
  endLine: number,
  content: string,
): string {
  return `Lines ${startLine}-${endLine} of file ${fileId}:\n\n${content}`;
}

// ── Status formatters ────────────────────────────────────────────────

export function formatStatusJSON(health: unknown, embeddings: unknown): string {
  return JSON.stringify({ health, embeddings }, null, 2);
}

export function formatStatusText(health: unknown, embeddings: unknown): string {
  return `## Health\n\`\`\`json\n${JSON.stringify(health, null, 2)}\n\`\`\`\n\n## Embeddings\n\`\`\`json\n${JSON.stringify(embeddings, null, 2)}\n\`\`\``;
}

// ── Error formatter ──────────────────────────────────────────────────

export function formatError(message: string): string {
  return JSON.stringify({ error: message });
}
