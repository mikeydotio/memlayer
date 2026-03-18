import type { LargeResponseRef } from "./api-client.js";

export function formatLargeResponseNotice(ref: LargeResponseRef): string {
  return [
    `The full response was too large to return inline (${ref.size_bytes} bytes, type: ${ref.content_type}). It has been saved to a file.`,
    ``,
    `**File ID:** \`${ref.file_id}\``,
    ``,
    `**Summary:**`,
    ref.summary,
    ``,
    `**Structural Index (line ranges):**`,
    ref.index,
    ``,
    `Use \`memlayer read-file ${ref.file_id} --start <start> --end <end>\` with the line ranges above to read the sections you need.`,
  ].join("\n");
}
