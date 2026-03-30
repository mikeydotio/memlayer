import json
import logging

import httpx

from ..config import settings

logger = logging.getLogger(__name__)

SYSTEM_PROMPT = """You are a knowledge extraction agent for a conversation memory system.
Given conversation entries between a developer and an AI coding assistant,
extract entities (concepts, decisions, bugs, tools, patterns) and
relationships between them.

Rules:
- Only extract entities that represent durable knowledge (not ephemeral chatter)
- For each entity: name, type, brief description (1 sentence), confidence (0.0-1.0)
- Entity types: concept, decision, bug, pattern, tool, library, architecture, file, person, project
- For each relationship: source entity name, target entity name, type, brief reason, confidence
- Relationship types: supports, contradicts, supersedes, depends_on, refines, implements, related_to, part_of, caused_by, resolved_by
- Mark decisions and bugs with high confidence only if clearly stated
- If an entity was mentioned in prior context, reference it by the same name (do not create duplicates)
- Prefer specific names over generic ones ("pgvector HNSW index" not "database index")
- Skip greetings, acknowledgments, and tool invocation noise
- Return empty lists if no meaningful entities or relationships are found

Respond with valid JSON only, no markdown fences:
{"entities": [{"name": "...", "type": "...", "description": "...", "confidence": 0.9}], "relationships": [{"source": "...", "target": "...", "type": "...", "reason": "...", "confidence": 0.8}]}"""


class ExtractionResult:
    def __init__(self, entities: list[dict], relationships: list[dict], tokens_used: int = 0):
        self.entities = entities
        self.relationships = relationships
        self.tokens_used = tokens_used


class ExtractionProvider:
    async def extract(self, context_entries: list[dict], new_entries: list[dict]) -> ExtractionResult:
        raise NotImplementedError


def _build_user_prompt(context_entries: list[dict], new_entries: list[dict]) -> str:
    parts = []
    if context_entries:
        parts.append("[PRIOR CONTEXT — already processed, for reference only]")
        for e in context_entries:
            parts.append(f"[{e['message_type']}] {e['raw_content'][:500]}")
        parts.append("")
    parts.append("[NEW ENTRIES — extract from these]")
    for e in new_entries:
        parts.append(f"[{e['message_type']}] {e['raw_content'][:2000]}")
    return "\n".join(parts)


def _parse_extraction(text: str) -> tuple[list[dict], list[dict]]:
    """Parse LLM JSON response into (entities, relationships)."""
    text = text.strip()
    # Strip markdown fences if present
    if text.startswith("```"):
        lines = text.split("\n")
        lines = [l for l in lines if not l.startswith("```")]
        text = "\n".join(lines).strip()
    try:
        data = json.loads(text)
    except json.JSONDecodeError:
        logger.warning("Failed to parse extraction JSON")
        return [], []

    entities = data.get("entities", [])
    relationships = data.get("relationships", [])

    # Validate entity structure
    valid_entities = []
    for e in entities:
        if isinstance(e, dict) and e.get("name") and e.get("type"):
            valid_entities.append({
                "name": str(e["name"])[:512],
                "type": str(e["type"])[:64],
                "description": str(e.get("description", ""))[:1024],
                "confidence": float(e.get("confidence", 0.8)),
            })

    # Validate relationship structure
    valid_rels = []
    for r in relationships:
        if isinstance(r, dict) and r.get("source") and r.get("target") and r.get("type"):
            valid_rels.append({
                "source": str(r["source"])[:512],
                "target": str(r["target"])[:512],
                "type": str(r["type"])[:64],
                "reason": str(r.get("reason", ""))[:1024],
                "confidence": float(r.get("confidence", 0.8)),
            })

    return valid_entities, valid_rels


class OpenAIExtractor(ExtractionProvider):
    def __init__(self):
        from openai import AsyncOpenAI
        self.client = AsyncOpenAI(api_key=settings.openai_api_key)
        self.model = settings.extraction_llm_model or "gpt-4o-mini"

    async def extract(self, context_entries: list[dict], new_entries: list[dict]) -> ExtractionResult:
        user_prompt = _build_user_prompt(context_entries, new_entries)
        resp = await self.client.chat.completions.create(
            model=self.model,
            messages=[
                {"role": "system", "content": SYSTEM_PROMPT},
                {"role": "user", "content": user_prompt},
            ],
            max_tokens=2000,
            temperature=0,
        )
        text = resp.choices[0].message.content or ""
        tokens = resp.usage.total_tokens if resp.usage else 0
        entities, relationships = _parse_extraction(text)
        return ExtractionResult(entities, relationships, tokens)


class AnthropicExtractor(ExtractionProvider):
    def __init__(self):
        self.api_key = settings.anthropic_api_key
        self.model = settings.extraction_llm_model or "claude-haiku-4-5-20251001"
        self.base_url = "https://api.anthropic.com/v1/messages"

    async def extract(self, context_entries: list[dict], new_entries: list[dict]) -> ExtractionResult:
        user_prompt = _build_user_prompt(context_entries, new_entries)
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
                    "max_tokens": 2000,
                    "system": SYSTEM_PROMPT,
                    "messages": [{"role": "user", "content": user_prompt}],
                },
            )
            resp.raise_for_status()
            data = resp.json()
            text = data["content"][0]["text"] if data.get("content") else ""
            tokens = data.get("usage", {}).get("input_tokens", 0) + data.get("usage", {}).get("output_tokens", 0)
        entities, relationships = _parse_extraction(text)
        return ExtractionResult(entities, relationships, tokens)


class OllamaExtractor(ExtractionProvider):
    def __init__(self):
        self.base_url = settings.ollama_base_url
        self.model = settings.extraction_llm_model or "llama3.2"

    async def extract(self, context_entries: list[dict], new_entries: list[dict]) -> ExtractionResult:
        user_prompt = _build_user_prompt(context_entries, new_entries)
        async with httpx.AsyncClient(timeout=120.0) as client:
            resp = await client.post(
                f"{self.base_url}/api/chat",
                json={
                    "model": self.model,
                    "messages": [
                        {"role": "system", "content": SYSTEM_PROMPT},
                        {"role": "user", "content": user_prompt},
                    ],
                    "stream": False,
                    "format": "json",
                },
            )
            resp.raise_for_status()
            data = resp.json()
            text = data.get("message", {}).get("content", "")
        entities, relationships = _parse_extraction(text)
        return ExtractionResult(entities, relationships, 0)


def get_extractor() -> ExtractionProvider | None:
    provider = settings.extraction_llm_provider
    if not provider:
        return None
    if provider == "openai":
        if not settings.openai_api_key:
            logger.warning("extraction_llm_provider=openai but no OPENAI_API_KEY")
            return None
        return OpenAIExtractor()
    elif provider == "anthropic":
        if not settings.anthropic_api_key:
            logger.warning("extraction_llm_provider=anthropic but no ANTHROPIC_API_KEY")
            return None
        return AnthropicExtractor()
    elif provider == "ollama":
        return OllamaExtractor()
    else:
        logger.warning(f"Unknown extraction_llm_provider: {provider}")
        return None
