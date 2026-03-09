export interface LargeResponseRef {
  schema_version: number;
  file_id: string;
  file_url: string;
  size_bytes: number;
  summary: string;
  index: string;
  content_type: string;
}

export interface SearchResult {
  id: number;
  session_id: string;
  message_type: string;
  content_type: string;
  raw_content: string;
  tool_name: string | null;
  created_at: string;
  project_path: string | null;
  fts_rank: number;
  vector_rank: number;
  rrf_score: number;
}

export interface SearchResponse {
  results: SearchResult[];
  total: number;
  query_embedding_ms: number;
  search_ms: number;
  large_response?: LargeResponseRef | null;
}

export interface SessionMessage {
  id: number;
  message_type: string;
  content_type: string;
  raw_content: string;
  tool_name: string | null;
  created_at: string;
}

export interface SessionSummary {
  session_id: string;
  project_path: string | null;
  slug: string | null;
  created_at: string;
  message_count: number;
  messages: SessionMessage[];
  large_response?: LargeResponseRef | null;
}

export class MemlayerClient {
  constructor(
    private baseUrl: string,
    private authToken: string,
  ) {}

  private headers(): Record<string, string> {
    const h: Record<string, string> = { "Content-Type": "application/json" };
    if (this.authToken) {
      h["Authorization"] = `Bearer ${this.authToken}`;
    }
    return h;
  }

  async search(params: {
    query: string;
    session_id?: string;
    project_path?: string;
    limit: number;
  }): Promise<SearchResponse> {
    const resp = await fetch(`${this.baseUrl}/search`, {
      method: "POST",
      headers: this.headers(),
      body: JSON.stringify(params),
    });
    if (!resp.ok) {
      throw new Error(`Search failed: ${resp.status} ${await resp.text()}`);
    }
    return (await resp.json()) as SearchResponse;
  }

  async getSessionSummary(
    sessionId: string,
    limit: number = 200,
  ): Promise<SessionSummary> {
    const resp = await fetch(
      `${this.baseUrl}/sessions/${sessionId}/summary?limit=${limit}`,
      { headers: this.headers() },
    );
    if (!resp.ok) {
      throw new Error(
        `Session summary failed: ${resp.status} ${await resp.text()}`,
      );
    }
    return (await resp.json()) as SessionSummary;
  }

  async downloadFile(fileId: string): Promise<string> {
    const resp = await fetch(`${this.baseUrl}/files/${fileId}`, {
      headers: this.headers(),
    });
    if (!resp.ok) {
      throw new Error(
        `File download failed: ${resp.status} ${await resp.text()}`,
      );
    }
    return await resp.text();
  }
}
