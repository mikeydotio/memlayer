import * as fs from "node:fs";
import * as path from "node:path";
import * as os from "node:os";

export class FileCache {
  private cacheDir: string;

  constructor(cacheDir?: string) {
    this.cacheDir =
      cacheDir ||
      process.env.MEMLAYER_CACHE_DIR ||
      path.join(os.homedir(), ".claude", "memlayer", "cache");
  }

  private ensureDir(): void {
    if (!fs.existsSync(this.cacheDir)) {
      fs.mkdirSync(this.cacheDir, { recursive: true });
    }
  }

  private filePath(fileId: string): string {
    return path.join(this.cacheDir, `${fileId}.txt`);
  }

  async ensureCached(
    fileId: string,
    downloadFn: () => Promise<string>,
  ): Promise<string> {
    this.ensureDir();
    const localPath = this.filePath(fileId);
    if (!fs.existsSync(localPath)) {
      const content = await downloadFn();
      fs.writeFileSync(localPath, content, "utf-8");
    }
    return localPath;
  }

  readLines(localPath: string, startLine: number, endLine: number): string {
    const content = fs.readFileSync(localPath, "utf-8");
    const lines = content.split("\n");
    // 1-indexed, inclusive
    const start = Math.max(1, startLine) - 1;
    const end = Math.min(lines.length, endLine);
    return lines.slice(start, end).join("\n");
  }
}
