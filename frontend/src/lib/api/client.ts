export interface HealthResponse {
  status?: string;
  [key: string]: unknown;
}

export async function fetchHealth(fetchFn: typeof fetch = fetch): Promise<HealthResponse> {
  const response = await fetchFn('/healthz', {
    credentials: 'same-origin',
    headers: {
      Accept: 'application/json'
    }
  });

  if (!response.ok) {
    throw new Error(`Health check failed with status ${response.status}`);
  }

  return (await response.json()) as HealthResponse;
}
