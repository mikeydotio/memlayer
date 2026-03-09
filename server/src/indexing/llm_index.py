import logging

import httpx

from ..config import settings

logger = logging.getLogger(__name__)

SYSTEM_PROMPT = """You are a content indexer. Given a large text document, produce:
1. A concise summary (max 400 chars) describing what this content is about.
2. A structural index (max 1800 chars) as a list of "L{line_number}: {description}" entries
   showing the key sections/topics and their line locations.

Respond in this exact format:
SUMMARY:
<your summary>

INDEX:
<your index>"""


class LLMIndexProvider:
    async def generate(self, content: str) -> tuple[str, str]:
        """Returns (summary, structural_index)."""
        raise NotImplementedError


class OpenAIIndexer(LLMIndexProvider):
    def __init__(self):
        from openai import AsyncOpenAI
        self.client = AsyncOpenAI(api_key=settings.openai_api_key)
        self.model = settings.index_llm_model or "gpt-4o-mini"

    async def generate(self, content: str) -> tuple[str, str]:
        # Truncate content to avoid token limits
        truncated = content[:30000] if len(content) > 30000 else content
        resp = await self.client.chat.completions.create(
            model=self.model,
            messages=[
                {"role": "system", "content": SYSTEM_PROMPT},
                {"role": "user", "content": truncated},
            ],
            max_tokens=1000,
            temperature=0,
        )
        return _parse_response(resp.choices[0].message.content or "")


class AnthropicIndexer(LLMIndexProvider):
    def __init__(self):
        self.api_key = settings.anthropic_api_key
        self.model = settings.index_llm_model or "claude-haiku-4-5-20251001"
        self.base_url = "https://api.anthropic.com/v1/messages"

    async def generate(self, content: str) -> tuple[str, str]:
        truncated = content[:30000] if len(content) > 30000 else content
        async with httpx.AsyncClient(timeout=60.0) as client:
            resp = await client.post(
                self.base_url,
                headers={
                    "x-api-key": self.api_key,
                    "anthropic-version": "2023-06-01",
                    "content-type": "application/json",
                },
                json={
                    "model": self.model,
                    "max_tokens": 1000,
                    "system": SYSTEM_PROMPT,
                    "messages": [{"role": "user", "content": truncated}],
                },
            )
            resp.raise_for_status()
            data = resp.json()
            text = data["content"][0]["text"] if data.get("content") else ""
            return _parse_response(text)


class OllamaIndexer(LLMIndexProvider):
    def __init__(self):
        self.base_url = settings.ollama_base_url
        self.model = settings.index_llm_model or "llama3.2"

    async def generate(self, content: str) -> tuple[str, str]:
        truncated = content[:15000] if len(content) > 15000 else content
        async with httpx.AsyncClient(timeout=120.0) as client:
            resp = await client.post(
                f"{self.base_url}/api/chat",
                json={
                    "model": self.model,
                    "messages": [
                        {"role": "system", "content": SYSTEM_PROMPT},
                        {"role": "user", "content": truncated},
                    ],
                    "stream": False,
                },
            )
            resp.raise_for_status()
            data = resp.json()
            text = data.get("message", {}).get("content", "")
            return _parse_response(text)


def get_llm_indexer() -> LLMIndexProvider | None:
    provider = settings.index_llm_provider
    if not provider:
        return None
    if provider == "openai":
        if not settings.openai_api_key:
            logger.warning("index_llm_provider=openai but no OPENAI_API_KEY")
            return None
        return OpenAIIndexer()
    elif provider == "anthropic":
        if not settings.anthropic_api_key:
            logger.warning("index_llm_provider=anthropic but no ANTHROPIC_API_KEY")
            return None
        return AnthropicIndexer()
    elif provider == "ollama":
        return OllamaIndexer()
    else:
        logger.warning(f"Unknown index_llm_provider: {provider}")
        return None


def _parse_response(text: str) -> tuple[str, str]:
    """Parse LLM response into (summary, index)."""
    summary = ""
    index = ""

    if "SUMMARY:" in text and "INDEX:" in text:
        parts = text.split("INDEX:", 1)
        summary_part = parts[0].split("SUMMARY:", 1)
        if len(summary_part) > 1:
            summary = summary_part[1].strip()
        index = parts[1].strip()
    else:
        # Best effort: use first half as summary
        summary = text[:500].strip()

    return summary[:500], index[:2000]
