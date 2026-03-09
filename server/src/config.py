from pydantic_settings import BaseSettings


class Settings(BaseSettings):
    database_url: str = "postgresql://memlayer:changeme@localhost:5432/memlayer"
    memlayer_auth_token: str = ""
    embedding_provider: str = "openai"  # "openai" or "ollama"
    openai_api_key: str = ""
    embedding_model: str = "text-embedding-3-small"
    embedding_dimensions: int = 1536
    ollama_base_url: str = "http://localhost:11434"
    embedding_batch_size: int = 100
    embedding_interval_secs: float = 5.0

    # Large response settings
    large_response_threshold_search: int = 5000  # chars
    large_response_threshold_session: int = 5000  # chars
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


settings = Settings()
