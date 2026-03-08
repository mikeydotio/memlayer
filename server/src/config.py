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


settings = Settings()
