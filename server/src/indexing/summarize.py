import logging

from ..config import settings
from .detect import detect_content_type
from .heuristic import index_markdown, index_code, index_json, index_text
from .llm_index import get_llm_indexer

logger = logging.getLogger(__name__)

_HEURISTIC_INDEXERS = {
    "markdown": index_markdown,
    "code": index_code,
    "json": index_json,
    "text": index_text,
}


async def generate_index(content: str, mode: str | None = None) -> tuple[str, str, str]:
    """Generate summary and structural index for content.

    Returns (summary, structural_index, detected_content_type).
    """
    if mode is None:
        mode = settings.index_mode

    content_type = detect_content_type(content)

    if mode == "llm-only":
        indexer = get_llm_indexer()
        if indexer:
            try:
                summary, index = await indexer.generate(content)
                return summary, index, content_type
            except Exception:
                logger.exception("LLM indexing failed, falling back to heuristic")
        # Fall through to heuristic if LLM unavailable

    # Heuristic indexing
    heuristic_fn = _HEURISTIC_INDEXERS.get(content_type, index_text)
    summary, structural_index = heuristic_fn(content)

    if mode == "hybrid" and len(structural_index) < 100:
        indexer = get_llm_indexer()
        if indexer:
            try:
                llm_summary, llm_index = await indexer.generate(content)
                if llm_index:
                    summary = llm_summary or summary
                    structural_index = llm_index
            except Exception:
                logger.exception("LLM hybrid indexing failed, using heuristic result")

    return summary, structural_index, content_type
