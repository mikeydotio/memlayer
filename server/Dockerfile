FROM python:3.12-slim

COPY --from=ghcr.io/astral-sh/uv:latest /uv /usr/local/bin/uv

WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends curl && rm -rf /var/lib/apt/lists/*

COPY pyproject.toml .
RUN uv pip install --system --no-cache .

ARG MEMLAYER_VERSION=0.0.0-dev

COPY src/ ./src/
COPY static/ ./static/

RUN echo "__version__ = \"${MEMLAYER_VERSION}\"" > src/_version.py

RUN groupadd -r app && useradd -r -g app app \
    && mkdir -p /data/response_files /data/backups \
    && chown -R app:app /data /app

USER app
EXPOSE 8420
CMD ["uvicorn", "src.main:app", "--host", "0.0.0.0", "--port", "8420"]
