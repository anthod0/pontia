export function optionalString(value) {
    return typeof value === "string" && value.trim().length > 0 ? value.trim() : undefined;
}
export function asRecord(value) {
    return value && typeof value === "object" && !Array.isArray(value) ? value : undefined;
}
export async function parseJsonResponse(response) {
    const text = await response.text().catch(() => "");
    if (!text)
        return null;
    try {
        return JSON.parse(text);
    }
    catch {
        return text;
    }
}
export function responseDataRecord(body) {
    return asRecord(asRecord(body)?.data);
}
export async function fetchJson(fetchImpl, url, token) {
    const response = await fetchImpl(url, { headers: { Authorization: `Bearer ${token}` } });
    const body = await parseJsonResponse(response);
    if (!response.ok)
        throw new Error(`${response.status} ${response.statusText}`);
    return body;
}
