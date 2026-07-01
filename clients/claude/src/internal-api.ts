export function optionalString(value: unknown): string | undefined {
  return typeof value === "string" && value.trim().length > 0 ? value.trim() : undefined;
}

export function asRecord(value: unknown): Record<string, unknown> | undefined {
  return value && typeof value === "object" && !Array.isArray(value) ? (value as Record<string, unknown>) : undefined;
}

export async function parseJsonResponse(response: Response): Promise<unknown> {
  const text = await response.text().catch(() => "");
  if (!text) return null;
  try {
    return JSON.parse(text);
  } catch {
    return text;
  }
}

export function responseDataRecord(body: unknown): Record<string, unknown> | undefined {
  return asRecord(asRecord(body)?.data);
}

export async function fetchJson(fetchImpl: typeof fetch, url: string, token: string): Promise<unknown> {
  const response = await fetchImpl(url, { headers: { Authorization: `Bearer ${token}` } });
  const body = await parseJsonResponse(response);
  if (!response.ok) throw new Error(`${response.status} ${response.statusText}`);
  return body;
}
