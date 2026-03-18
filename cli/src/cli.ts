#!/usr/bin/env node
import { Command } from "commander";
import * as fs from "node:fs";
import * as path from "node:path";
import * as os from "node:os";
import { MemlayerClient } from "./api-client.js";
import { FileCache } from "./file-cache.js";
import {
  formatSearchJSON,
  formatSearchText,
  formatSessionJSON,
  formatSessionText,
  formatReadFileJSON,
  formatReadFileText,
  formatStatusJSON,
  formatStatusText,
  formatError,
} from "./cli-formatters.js";

// ── Config loading ───────────────────────────────────────────────────

function loadConfig(): { serverUrl: string; authToken: string } {
  let serverUrl = process.env.MEMLAYER_SERVER_URL || "";
  let authToken = process.env.MEMLAYER_AUTH_TOKEN || "";

  if (!serverUrl || !authToken) {
    const envFile = path.join(os.homedir(), ".config", "memlayer", "env");
    if (fs.existsSync(envFile)) {
      const lines = fs.readFileSync(envFile, "utf-8").split("\n");
      for (const line of lines) {
        const trimmed = line.trim();
        if (!trimmed || trimmed.startsWith("#")) continue;
        const eq = trimmed.indexOf("=");
        if (eq < 0) continue;
        const key = trimmed.slice(0, eq);
        const val = trimmed.slice(eq + 1);
        if (key === "MEMLAYER_SERVER_URL" && !serverUrl) serverUrl = val;
        if (key === "MEMLAYER_AUTH_TOKEN" && !authToken) authToken = val;
      }
    }
  }

  if (!serverUrl) serverUrl = "http://localhost:8420/api";

  return { serverUrl, authToken };
}

function createClient(): MemlayerClient {
  const { serverUrl, authToken } = loadConfig();
  return new MemlayerClient(serverUrl, authToken);
}

// ── Error handling ───────────────────────────────────────────────────

function handleError(e: unknown): never {
  const message = e instanceof Error ? e.message : String(e);
  process.stderr.write(formatError(message) + "\n");
  process.exit(1);
}

// ── CLI definition ───────────────────────────────────────────────────

const program = new Command();

program
  .name("memlayer")
  .description("Memlayer — search and recall past Claude Code conversations")
  .version("1.5.0");

// ── search ───────────────────────────────────────────────────────────

program
  .command("search")
  .description("Search across all past Claude Code conversations")
  .argument("<query>", "Natural language search query")
  .option("--project <path>", "Filter to project path")
  .option("--session-id <uuid>", "Filter to specific session")
  .option("--limit <n>", "Max results (1-50)", "10")
  .option("--after <iso8601>", "Entries after timestamp")
  .option("--before <iso8601>", "Entries before timestamp")
  .option("--types <types>", "Comma-separated: user,assistant,tool_use,tool_result")
  .option("--format <fmt>", "Output format: json or text", "json")
  .action(async (query: string, opts) => {
    try {
      const client = createClient();
      const fileCache = new FileCache();

      const types = opts.types
        ? opts.types.split(",").map((t: string) => t.trim())
        : undefined;

      const results = await client.search({
        query,
        session_id: opts.sessionId,
        project_path: opts.project,
        limit: parseInt(opts.limit, 10),
        after: opts.after,
        before: opts.before,
        types,
      });

      // Pre-cache large response file if present
      if (results.large_response) {
        await fileCache.ensureCached(results.large_response.file_id, () =>
          client.downloadFile(results.large_response!.file_id),
        );
      }

      const output =
        opts.format === "text"
          ? formatSearchText(results)
          : formatSearchJSON(results);
      process.stdout.write(output + "\n");
    } catch (e) {
      handleError(e);
    }
  });

// ── session ──────────────────────────────────────────────────────────

program
  .command("session")
  .description("Retrieve full conversation history for a session")
  .argument("<session_id>", "Session UUID to retrieve")
  .option("--limit <n>", "Max entries (1-500)", "200")
  .option("--types <types>", "Comma-separated: user,assistant,tool_use,tool_result")
  .option("--format <fmt>", "Output format: json or text", "json")
  .action(async (sessionId: string, opts) => {
    try {
      const client = createClient();
      const fileCache = new FileCache();

      const types = opts.types
        ? opts.types.split(",").map((t: string) => t.trim())
        : undefined;

      const summary = await client.getSessionSummary(
        sessionId,
        parseInt(opts.limit, 10),
        types,
      );

      // Pre-cache large response file if present
      if (summary.large_response) {
        await fileCache.ensureCached(summary.large_response.file_id, () =>
          client.downloadFile(summary.large_response!.file_id),
        );
      }

      const output =
        opts.format === "text"
          ? formatSessionText(summary)
          : formatSessionJSON(summary);
      process.stdout.write(output + "\n");
    } catch (e) {
      handleError(e);
    }
  });

// ── read-file ────────────────────────────────────────────────────────

program
  .command("read-file")
  .description("Read a line range from a large response file")
  .argument("<file_id>", "File ID from large_response reference")
  .option("--start <n>", "Start line (1-indexed, inclusive)")
  .option("--end <n>", "End line (1-indexed, inclusive)")
  .option("--format <fmt>", "Output format: json or text", "json")
  .action(async (fileId: string, opts) => {
    try {
      if (!opts.start || !opts.end) {
        process.stderr.write(
          formatError("--start and --end are required") + "\n",
        );
        process.exit(1);
      }

      const client = createClient();
      const fileCache = new FileCache();

      const startLine = parseInt(opts.start, 10);
      const endLine = parseInt(opts.end, 10);

      const localPath = await fileCache.ensureCached(fileId, () =>
        client.downloadFile(fileId),
      );
      const content = fileCache.readLines(localPath, startLine, endLine);

      const output =
        opts.format === "text"
          ? formatReadFileText(fileId, startLine, endLine, content)
          : formatReadFileJSON(fileId, startLine, endLine, content);
      process.stdout.write(output + "\n");
    } catch (e) {
      handleError(e);
    }
  });

// ── status ───────────────────────────────────────────────────────────

program
  .command("status")
  .description("Show server health and embedding status")
  .option("--format <fmt>", "Output format: json or text", "json")
  .action(async (opts) => {
    try {
      const { serverUrl, authToken } = loadConfig();
      const healthUrl = serverUrl.replace(/\/api\/?$/, "") + "/health";
      const embeddingsUrl = serverUrl.replace(/\/api\/?$/, "") + "/api/embeddings/status";

      const headers: Record<string, string> = {
        "Content-Type": "application/json",
      };
      if (authToken) {
        headers["Authorization"] = `Bearer ${authToken}`;
      }

      const [healthResp, embeddingsResp] = await Promise.allSettled([
        fetch(healthUrl, { headers }),
        fetch(embeddingsUrl, { headers }),
      ]);

      const health =
        healthResp.status === "fulfilled" && healthResp.value.ok
          ? await healthResp.value.json()
          : { error: healthResp.status === "rejected" ? healthResp.reason?.message : "unreachable" };

      const embeddings =
        embeddingsResp.status === "fulfilled" && embeddingsResp.value.ok
          ? await embeddingsResp.value.json()
          : { error: embeddingsResp.status === "rejected" ? embeddingsResp.reason?.message : "unreachable" };

      const output =
        opts.format === "text"
          ? formatStatusText(health, embeddings)
          : formatStatusJSON(health, embeddings);
      process.stdout.write(output + "\n");
    } catch (e) {
      handleError(e);
    }
  });

program.parse();
