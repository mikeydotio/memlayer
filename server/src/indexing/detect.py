import json
import re


def detect_content_type(content: str) -> str:
    """Detect content type: 'markdown', 'code', 'json', or 'text'."""
    stripped = content.strip()
    # JSON: check if starts with { or [ and try parsing full content
    if stripped.startswith(("{", "[")):
        try:
            json.loads(content)
            return "json"
        except (json.JSONDecodeError, ValueError):
            pass

    lines = content.split("\n")[:50]

    # Markdown: 2+ heading patterns in first 50 lines
    heading_count = sum(1 for line in lines if re.match(r"^#{1,6}\s", line))
    if heading_count >= 2:
        return "markdown"

    # Code: 3+ function/class signatures in first 50 lines
    code_patterns = re.compile(
        r"^\s*(def |fn |function |class |impl |struct |const |let |var |export |import |pub )"
    )
    code_count = sum(1 for line in lines if code_patterns.match(line))
    if code_count >= 3:
        return "code"

    return "text"
