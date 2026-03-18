import * as fs from "node:fs";
import * as path from "node:path";
import * as os from "node:os";

const SOFT_LIMIT = 50 * 1024 * 1024; // 50MB — triggers async background eviction
const HARD_LIMIT = 100 * 1024 * 1024; // 100MB — triggers sync eviction before write

interface CacheEntry {
  filePath: string;
  size: number;
  mtimeMs: number;
}

export class FileCache {
  private cacheDir: string;
  private currentSize: number | null = null; // lazy-initialized on first access

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

  /** Scan cache directory and return entries sorted by mtime (oldest first = FIFO). */
  private scanEntries(): CacheEntry[] {
    this.ensureDir();
    const entries: CacheEntry[] = [];
    try {
      for (const name of fs.readdirSync(this.cacheDir)) {
        const filePath = path.join(this.cacheDir, name);
        try {
          const stat = fs.statSync(filePath);
          if (stat.isFile()) {
            entries.push({ filePath, size: stat.size, mtimeMs: stat.mtimeMs });
          }
        } catch {
          // File may have been removed between readdir and stat
        }
      }
    } catch {
      // Directory read failure — treat as empty
    }
    entries.sort((a, b) => a.mtimeMs - b.mtimeMs); // oldest first
    return entries;
  }

  /** Get total cache size, scanning on first access. */
  private getCacheSize(): number {
    if (this.currentSize === null) {
      const entries = this.scanEntries();
      this.currentSize = entries.reduce((sum, e) => sum + e.size, 0);
    }
    return this.currentSize;
  }

  /** FIFO eviction: remove oldest files until cache is at or below targetBytes. */
  private evictTo(targetBytes: number): void {
    const entries = this.scanEntries();
    let totalSize = entries.reduce((sum, e) => sum + e.size, 0);

    for (const entry of entries) {
      if (totalSize <= targetBytes) break;
      try {
        fs.unlinkSync(entry.filePath);
        totalSize -= entry.size;
      } catch {
        // File already removed
      }
    }
    this.currentSize = totalSize;
  }

  /** Schedule async background eviction to soft limit. */
  private scheduleEviction(): void {
    setImmediate(() => {
      try {
        this.evictTo(SOFT_LIMIT);
      } catch {
        // Best-effort background eviction
      }
    });
  }

  async ensureCached(
    fileId: string,
    downloadFn: () => Promise<string>,
  ): Promise<string> {
    this.ensureDir();
    const localPath = this.filePath(fileId);

    // Check-before-write: skip download if already cached
    if (fs.existsSync(localPath)) {
      return localPath;
    }

    const content = await downloadFn();
    const contentBytes = Buffer.byteLength(content, "utf-8");

    // Hard limit: sync eviction to make room for the new file
    const cacheSize = this.getCacheSize();
    if (cacheSize + contentBytes > HARD_LIMIT) {
      this.evictTo(HARD_LIMIT - contentBytes);
    }

    fs.writeFileSync(localPath, content, "utf-8");
    this.currentSize = (this.currentSize ?? 0) + contentBytes;

    // Soft limit: async background eviction
    if (this.getCacheSize() > SOFT_LIMIT) {
      this.scheduleEviction();
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
