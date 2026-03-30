from pydantic_settings import BaseSettings


class Settings(BaseSettings):
    database_url: str = "postgresql://memlayer:changeme@localhost:5432/memlayer"
    memlayer_auth_token: str = ""
    embedding_provider: str = "openai"  # "openai" or "ollama"
    openai_api_key: str = ""
    embedding_model: str = "text-embedding-3-small"
    embedding_dimensions: int = 1536
    ollama_base_url: str = "http://localhost:11434"
    embedding_batch_size: int = 20
    embedding_interval_secs: float = 5.0

    # Large response settings
    response_budget_bytes: int = 200000  # 200KB; responses exceeding this use file-based flow
    file_storage_path: str = "/data/response_files"
    file_storage_soft_limit: int = 0  # bytes, 0 = unlimited
    file_storage_hard_limit: int = 0  # bytes, 0 = unlimited
    max_file_size: int = 0  # bytes, 0 = unlimited
    max_db_size: int = 0  # bytes, 0 = unlimited
    eviction_interval_secs: float = 60.0

    # Indexing settings
    index_mode: str = "off"  # "off" | "hybrid" | "llm-only"
    index_llm_provider: str = ""  # "openai" | "anthropic" | "ollama"
    index_llm_model: str = ""
    anthropic_api_key: str = ""

    # Knowledge graph extraction settings
    extraction_mode: str = "off"  # "off" | "auto" | "on"
    extraction_llm_provider: str = ""  # "openai" | "anthropic" | "ollama"
    extraction_llm_model: str = ""
    extraction_batch_size: int = 10  # entries per LLM call
    extraction_context_window: int = 20  # prior entries for session context
    extraction_interval_secs: float = 10.0
    extraction_confidence_threshold: float = 0.5
    entity_resolution_threshold: float = 0.7

    # Migration settings
    migration_key_ttl_secs: int = 3600  # 1 hour initial TTL
    migration_batch_size: int = 200
    server_id: str = ""  # Auto-generated UUID if empty

    # Logging settings
    log_format: str = "text"  # "text" or "json"
    log_level: str = "INFO"


settings = Settings()
