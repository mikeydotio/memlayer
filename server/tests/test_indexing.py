"""Unit tests for indexing: content detection and heuristic indexers."""

import json

import pytest

from src.indexing.detect import detect_content_type
from src.indexing.heuristic import (
    index_markdown,
    index_code,
    index_json,
    index_text,
    _fallback_summary,
)


class TestDetectContentType:
    """Tests for detect_content_type()."""

    def test_detect_markdown_with_headings(self):
        """Content with 2+ markdown headings should be detected as markdown."""
        content = "# Title\n\nSome text\n\n## Section 1\n\nMore text\n\n## Section 2\n"
        assert detect_content_type(content) == "markdown"

    def test_detect_markdown_needs_two_headings(self):
        """Single heading is not enough for markdown detection."""
        content = "# Just One Heading\n\nSome paragraph text here."
        # With only 1 heading and no code patterns, it falls to text
        assert detect_content_type(content) == "text"

    def test_detect_code_with_multiple_definitions(self):
        """Content with 3+ function/class signatures should be detected as code."""
        content = (
            "def foo():\n    pass\n\n"
            "def bar():\n    pass\n\n"
            "class Baz:\n    pass\n"
        )
        assert detect_content_type(content) == "code"

    def test_detect_code_needs_three_signatures(self):
        """Two signatures are not enough for code detection."""
        content = "def foo():\n    pass\n\ndef bar():\n    pass\n"
        assert detect_content_type(content) == "text"

    def test_detect_code_various_keywords(self):
        """Various language keywords should trigger code detection."""
        content = (
            "fn main() {\n}\n\n"
            "struct Config {\n}\n\n"
            "impl Config {\n}\n"
        )
        assert detect_content_type(content) == "code"

    def test_detect_json_object(self):
        """Valid JSON object should be detected."""
        content = json.dumps({"key": "value", "num": 42})
        assert detect_content_type(content) == "json"

    def test_detect_json_array(self):
        """Valid JSON array should be detected."""
        content = json.dumps([1, 2, 3, "hello"])
        assert detect_content_type(content) == "json"

    def test_detect_json_invalid(self):
        """Invalid JSON starting with { should not be detected as json."""
        content = "{this is not json: [incomplete"
        result = detect_content_type(content)
        assert result != "json"  # falls through to text or other

    def test_detect_text_plain(self):
        """Plain text without special patterns should be detected as text."""
        content = "This is just some plain text.\nNothing special here.\n"
        assert detect_content_type(content) == "text"

    def test_detect_text_empty(self):
        """Empty string should be detected as text."""
        assert detect_content_type("") == "text"

    def test_detect_markdown_before_code(self):
        """If content has both headings and code, markdown wins (checked first)."""
        content = (
            "# API Reference\n\n"
            "## Functions\n\n"
            "def foo():\n    pass\n\n"
            "def bar():\n    pass\n\n"
            "def baz():\n    pass\n"
        )
        assert detect_content_type(content) == "markdown"

    def test_detect_json_before_markdown(self):
        """JSON check happens before markdown check."""
        content = json.dumps({"# Heading": "value", "## Another": "heading"})
        assert detect_content_type(content) == "json"


class TestIndexMarkdown:
    """Tests for index_markdown()."""

    def test_heading_extraction(self):
        """Should extract headings with line numbers and levels."""
        content = "# Title\n\nSome text\n\n## Section A\n\nMore\n\n### Subsection\n"
        summary, index = index_markdown(content)
        assert "Title" in summary
        assert "Section A" in summary or "3 sections" in summary
        assert "L1:" in index
        assert "Title" in index
        assert "L5:" in index
        assert "Section A" in index
        assert "L9:" in index
        assert "Subsection" in index

    def test_heading_indentation(self):
        """Deeper headings should have more indentation in index."""
        content = "# Top\n\n## Mid\n\n### Deep\n"
        _, index = index_markdown(content)
        lines = index.strip().split("\n")
        # Level 1 = no indent, Level 2 = 2 spaces, Level 3 = 4 spaces
        assert lines[0].startswith("L1: Top") or lines[0].startswith("L1:   Top") is False
        assert "  Mid" in lines[1]
        assert "    Deep" in lines[2]

    def test_no_headings_fallback(self):
        """Content without headings should use fallback summary."""
        content = "Just some text without any headings.\nLine two.\n"
        summary, index = index_markdown(content)
        assert "lines" in summary or "chars" in summary  # fallback format
        assert index == ""

    def test_summary_length_limit(self):
        """Summary should be truncated to 500 chars max."""
        headings = "\n\n".join([f"## {'A' * 60} {i}" for i in range(20)])
        content = "# Main Title\n\n" + headings
        summary, _ = index_markdown(content)
        assert len(summary) <= 500

    def test_index_length_limit(self):
        """Structural index should be truncated to 2000 chars max."""
        headings = "\n\n".join([f"## {'LongHeading' * 10} {i}" for i in range(100)])
        content = "# Title\n\n" + headings
        _, index = index_markdown(content)
        assert len(index) <= 2000


class TestIndexCode:
    """Tests for index_code()."""

    def test_function_signature_extraction(self):
        """Should extract function signatures with line numbers."""
        content = (
            "import os\n\n"
            "def hello(name: str):\n"
            "    print(name)\n\n"
            "def goodbye():\n"
            "    pass\n"
        )
        summary, index = index_code(content)
        assert "hello" in summary or "hello" in index
        assert "goodbye" in summary or "goodbye" in index
        assert "L3:" in index
        assert "L6:" in index

    def test_class_extraction(self):
        """Should extract class definitions."""
        content = (
            "class Animal:\n    pass\n\n"
            "class Dog(Animal):\n    pass\n\n"
            "class Cat(Animal):\n    pass\n"
        )
        summary, index = index_code(content)
        assert "3 definitions" in summary
        assert "Animal" in summary
        assert "Dog" in summary or "Dog" in index

    def test_mixed_languages(self):
        """Should detect signatures from multiple languages."""
        content = (
            "fn rust_function() {\n}\n\n"
            "pub fn public_fn() {\n}\n\n"
            "struct MyStruct {\n}\n"
        )
        summary, index = index_code(content)
        assert "3 definitions" in summary

    def test_long_signature_truncation(self):
        """Signatures over 120 chars should be truncated."""
        long_name = "a" * 150
        content = f"def {long_name}():\n    pass\n\ndef b():\n    pass\n\ndef c():\n    pass\n"
        _, index = index_code(content)
        # Check that no single line exceeds reasonable length
        for line in index.split("\n"):
            # L{num}: prefix + 120 chars + "..."
            assert len(line) < 200

    def test_no_signatures_fallback(self):
        """Code without recognized signatures should use fallback."""
        content = "x = 1\ny = 2\nprint(x + y)\n"
        summary, index = index_code(content)
        assert "lines" in summary or "chars" in summary
        assert index == ""

    def test_export_patterns(self):
        """JavaScript export patterns should be recognized."""
        content = (
            "export function doThing() {\n}\n\n"
            "export class Widget {\n}\n\n"
            "export const API_KEY = 'abc'\n"
        )
        summary, index = index_code(content)
        assert "3 definitions" in summary


class TestIndexJson:
    """Tests for index_json()."""

    def test_json_object_keys(self):
        """Should list top-level keys with types."""
        data = {"name": "Alice", "age": 30, "items": [1, 2, 3], "meta": {"k": "v"}}
        content = json.dumps(data)
        summary, index = index_json(content)
        assert "4 top-level keys" in summary
        assert "name" in summary
        assert "name: str" in index
        assert "age: int" in index
        assert "items: list (len=3)" in index
        assert "meta: dict (keys=1)" in index

    def test_json_array(self):
        """Should describe array length and element type."""
        content = json.dumps([1, 2, 3, 4, 5])
        summary, index = index_json(content)
        assert "5 elements" in summary
        assert "int" in index

    def test_json_invalid_fallback(self):
        """Invalid JSON should produce a fallback summary."""
        content = "{not valid json"
        summary, index = index_json(content)
        assert "lines" in summary or "chars" in summary
        assert index == ""

    def test_json_large_object(self):
        """Large JSON object should only list first 50 keys in index."""
        data = {f"key_{i}": i for i in range(100)}
        content = json.dumps(data)
        summary, index = index_json(content)
        assert "100 top-level keys" in summary
        # Index should contain at most 50 entries (plus truncation)
        index_lines = [l for l in index.strip().split("\n") if l.strip()]
        assert len(index_lines) <= 50

    def test_json_empty_object(self):
        """Empty JSON object."""
        content = json.dumps({})
        summary, index = index_json(content)
        assert "0 top-level keys" in summary

    def test_json_empty_array(self):
        """Empty JSON array."""
        content = json.dumps([])
        summary, index = index_json(content)
        assert "0 elements" in summary

    def test_json_string_value_length(self):
        """String values should show length."""
        data = {"description": "A fairly long description text"}
        content = json.dumps(data)
        _, index = index_json(content)
        assert "str" in index
        assert "len=" in index

    def test_json_scalar(self):
        """JSON scalar value (number, string, etc)."""
        content = json.dumps(42)
        summary, index = index_json(content)
        assert "scalar" in summary.lower()


class TestIndexText:
    """Tests for index_text()."""

    def test_paragraph_extraction(self):
        """Should extract paragraph boundaries with first-sentence previews."""
        content = (
            "First paragraph here. It has details.\n\n"
            "Second paragraph starts. More info follows.\n\n"
            "Third paragraph. The last one.\n"
        )
        summary, index = index_text(content)
        assert "3 paragraphs" in summary
        assert "First paragraph here" in index
        assert "Second paragraph starts" in index
        assert "Third paragraph" in index

    def test_paragraph_line_numbers(self):
        """Each paragraph entry should have L{num}: prefix."""
        content = "Para one.\n\nPara two.\n\nPara three.\n"
        _, index = index_text(content)
        assert "L1:" in index
        # Para two starts after line 1 + blank line
        assert "L3:" in index

    def test_long_first_sentence_truncation(self):
        """First sentences longer than 100 chars should be truncated."""
        long_sentence = "A" * 150 + ". Second sentence."
        content = long_sentence + "\n\nAnother paragraph.\n"
        _, index = index_text(content)
        # Should contain "..." for truncated sentences
        assert "..." in index

    def test_empty_content(self):
        """Empty content should produce fallback."""
        summary, index = index_text("")
        assert "lines" in summary or "chars" in summary
        assert index == ""

    def test_summary_from_first_paragraph(self):
        """Summary should begin with the first paragraph content."""
        content = "The quick brown fox jumped. Over the lazy dog.\n\nSecond para.\n"
        summary, _ = index_text(content)
        assert "The quick brown fox" in summary

    def test_max_paragraphs(self):
        """Should process at most 30 paragraphs in index."""
        paragraphs = "\n\n".join([f"Paragraph number {i}." for i in range(50)])
        _, index = index_text(paragraphs)
        index_lines = [l for l in index.strip().split("\n") if l.strip()]
        assert len(index_lines) <= 30


class TestFallbackSummary:
    """Tests for _fallback_summary()."""

    def test_fallback_summary_format(self):
        """Should contain line count, char count, and preview."""
        content = "Line one.\nLine two.\nLine three."
        summary = _fallback_summary(content)
        assert "3 lines" in summary
        assert "chars" in summary
        assert "Preview:" in summary

    def test_fallback_summary_truncation(self):
        """Long content preview should be truncated with ..."""
        content = "A" * 300
        summary = _fallback_summary(content)
        assert "..." in summary
        assert len(summary) <= 500
