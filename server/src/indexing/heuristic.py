import json
import re


def index_markdown(content: str) -> tuple[str, str]:
    """Extract heading tree with line numbers from markdown content."""
    lines = content.split("\n")
    headings = []
    for i, line in enumerate(lines, 1):
        m = re.match(r"^(#{1,6})\s+(.+)", line)
        if m:
            level = len(m.group(1))
            title = m.group(2).strip()
            headings.append((i, level, title))

    if not headings:
        return _fallback_summary(content), ""

    # Build structural index
    index_lines = []
    for line_num, level, title in headings:
        indent = "  " * (level - 1)
        index_lines.append(f"L{line_num}: {indent}{title}")

    structural_index = "\n".join(index_lines)[:2000]

    # Build summary from top-level headings
    top_headings = [t for _, l, t in headings if l <= 2][:10]
    summary = f"Markdown document with {len(headings)} sections. "
    if top_headings:
        summary += "Topics: " + ", ".join(top_headings)
    summary = summary[:500]

    return summary, structural_index


def index_code(content: str) -> tuple[str, str]:
    """Extract function/class signatures with line numbers."""
    lines = content.split("\n")
    sig_pattern = re.compile(
        r"^\s*(def |fn |function |class |impl |struct |pub fn |pub struct |"
        r"export (function|class|const|default) |async function |async def )"
    )

    signatures = []
    for i, line in enumerate(lines, 1):
        if sig_pattern.match(line):
            sig = line.strip()
            if len(sig) > 120:
                sig = sig[:120] + "..."
            signatures.append((i, sig))

    if not signatures:
        return _fallback_summary(content), ""

    index_lines = [f"L{line_num}: {sig}" for line_num, sig in signatures]
    structural_index = "\n".join(index_lines)[:2000]

    summary = f"Code with {len(signatures)} definitions. "
    names = []
    for _, sig in signatures[:10]:
        # Extract just the name
        m = re.search(r"(?:def|fn|function|class|struct|impl)\s+(\w+)", sig)
        if m:
            names.append(m.group(1))
    if names:
        summary += "Defines: " + ", ".join(names)
    summary = summary[:500]

    return summary, structural_index


def index_json(content: str) -> tuple[str, str]:
    """Extract top-level keys with types and array lengths."""
    try:
        data = json.loads(content)
    except (json.JSONDecodeError, ValueError):
        return _fallback_summary(content), ""

    if isinstance(data, dict):
        index_lines = []
        for key, value in list(data.items())[:50]:
            type_name = type(value).__name__
            extra = ""
            if isinstance(value, list):
                extra = f" (len={len(value)})"
            elif isinstance(value, dict):
                extra = f" (keys={len(value)})"
            elif isinstance(value, str):
                extra = f" (len={len(value)})"
            index_lines.append(f"{key}: {type_name}{extra}")

        structural_index = "\n".join(index_lines)[:2000]
        summary = f"JSON object with {len(data)} top-level keys: {', '.join(list(data.keys())[:10])}"
    elif isinstance(data, list):
        structural_index = f"JSON array with {len(data)} elements"
        if data:
            structural_index += f"\nElement type: {type(data[0]).__name__}"
        summary = f"JSON array with {len(data)} elements"
    else:
        structural_index = f"JSON scalar: {type(data).__name__}"
        summary = f"JSON scalar value ({type(data).__name__})"

    return summary[:500], structural_index[:2000]


def index_text(content: str) -> tuple[str, str]:
    """Extract paragraph boundaries with first-sentence previews."""
    paragraphs = re.split(r"\n\s*\n", content)
    paragraphs = [p.strip() for p in paragraphs if p.strip()]

    if not paragraphs:
        return _fallback_summary(content), ""

    index_lines = []
    char_offset = 0
    line_num = 1
    lines = content.split("\n")

    for para in paragraphs[:30]:
        # Find line number of this paragraph
        para_start = content.find(para, char_offset)
        if para_start >= 0:
            line_num = content[:para_start].count("\n") + 1
            char_offset = para_start + len(para)

        # First sentence preview
        first_sentence = para.split(".")[0].strip()
        if len(first_sentence) > 100:
            first_sentence = first_sentence[:100] + "..."
        index_lines.append(f"L{line_num}: {first_sentence}")

    structural_index = "\n".join(index_lines)[:2000]

    # Summary from first paragraph
    first_para = paragraphs[0]
    if len(first_para) > 400:
        first_para = first_para[:400] + "..."
    summary = f"Text with {len(paragraphs)} paragraphs. Begins: {first_para}"
    summary = summary[:500]

    return summary, structural_index


def _fallback_summary(content: str) -> str:
    """Generate a minimal summary when specific indexing fails."""
    lines = content.split("\n")
    char_count = len(content)
    preview = content[:200].replace("\n", " ").strip()
    if len(content) > 200:
        preview += "..."
    return f"{len(lines)} lines, {char_count} chars. Preview: {preview}"[:500]
