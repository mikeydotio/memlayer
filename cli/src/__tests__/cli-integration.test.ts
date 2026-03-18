import { describe, it, expect } from "vitest";
import { execFileSync } from "node:child_process";
import * as path from "node:path";

const CLI_PATH = path.join(import.meta.dirname, "..", "..", "dist", "cli.js");

function run(
  args: string[],
  env: Record<string, string> = {},
): { stdout: string; stderr: string; exitCode: number } {
  try {
    const stdout = execFileSync("node", [CLI_PATH, ...args], {
      env: { ...process.env, ...env },
      encoding: "utf-8",
      timeout: 10_000,
    });
    return { stdout, stderr: "", exitCode: 0 };
  } catch (e: unknown) {
    const err = e as { stdout?: string; stderr?: string; status?: number };
    return {
      stdout: err.stdout || "",
      stderr: err.stderr || "",
      exitCode: err.status ?? 1,
    };
  }
}

describe("CLI integration", () => {
  it("shows help with --help", () => {
    const { stdout, exitCode } = run(["--help"]);
    expect(exitCode).toBe(0);
    expect(stdout).toContain("memlayer");
    expect(stdout).toContain("search");
    expect(stdout).toContain("session");
    expect(stdout).toContain("read-file");
    expect(stdout).toContain("status");
  });

  it("shows version with --version", () => {
    const { stdout, exitCode } = run(["--version"]);
    expect(exitCode).toBe(0);
    expect(stdout).toContain("1.5.0");
  });

  it("search errors with JSON on stderr when server unreachable", () => {
    const { stderr, exitCode } = run(["search", "test query"], {
      MEMLAYER_SERVER_URL: "http://127.0.0.1:1",
      MEMLAYER_AUTH_TOKEN: "test",
    });
    expect(exitCode).toBe(1);
    const parsed = JSON.parse(stderr.trim());
    expect(parsed.error).toBeTruthy();
  });

  it("session errors with JSON on stderr when server unreachable", () => {
    const { stderr, exitCode } = run(
      ["session", "11111111-2222-3333-4444-555555555555"],
      {
        MEMLAYER_SERVER_URL: "http://127.0.0.1:1",
        MEMLAYER_AUTH_TOKEN: "test",
      },
    );
    expect(exitCode).toBe(1);
    const parsed = JSON.parse(stderr.trim());
    expect(parsed.error).toBeTruthy();
  });

  it("read-file requires --start and --end", () => {
    const { stderr, exitCode } = run(
      ["read-file", "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"],
      {
        MEMLAYER_SERVER_URL: "http://127.0.0.1:1",
        MEMLAYER_AUTH_TOKEN: "test",
      },
    );
    expect(exitCode).toBe(1);
    const parsed = JSON.parse(stderr.trim());
    expect(parsed.error).toContain("--start and --end are required");
  });

  it("status outputs JSON even when server is unreachable", () => {
    const { stdout, exitCode } = run(["status"], {
      MEMLAYER_SERVER_URL: "http://127.0.0.1:1",
      MEMLAYER_AUTH_TOKEN: "test",
    });
    expect(exitCode).toBe(0);
    const parsed = JSON.parse(stdout.trim());
    expect(parsed.health).toBeDefined();
    expect(parsed.embeddings).toBeDefined();
  });
});
