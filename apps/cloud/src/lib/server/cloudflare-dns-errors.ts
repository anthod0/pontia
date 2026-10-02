export type DnsOperation = "lookup" | "create" | "update" | "delete";
export type DnsErrorCategory =
  | "network_error"
  | "provider_http_error"
  | "provider_rejected"
  | "invalid_provider_response"
  | "local_validation_error";

export type ProviderError = { code: string | number; message: string };

export class DnsProviderError extends Error {
  constructor(
    readonly operation: DnsOperation,
    readonly category: DnsErrorCategory,
    readonly status?: number,
    readonly providerErrors?: ProviderError[],
  ) {
    super(`dns_${operation}_${category}`);
    this.name = "DnsProviderError";
  }

  fields(): Record<string, unknown> {
    return {
      operation: this.operation,
      error: this.category,
      ...(this.status === undefined ? {} : { status: this.status }),
      ...(this.providerErrors?.length ? { provider_errors: this.providerErrors } : {}),
    };
  }
}

const MAX_PROVIDER_RESPONSE_BYTES = 16_384;
const MAX_PROVIDER_ERRORS = 5;
const MAX_PROVIDER_CODE_LENGTH = 64;
const MAX_PROVIDER_MESSAGE_LENGTH = 256;

async function boundedProviderJson(response: Response): Promise<unknown> {
  const declared = response.headers.get("content-length");
  if (declared !== null && Number(declared) > MAX_PROVIDER_RESPONSE_BYTES) throw new Error();
  if (!response.body) return JSON.parse("") as unknown;
  const reader = response.body.getReader();
  const chunks: Uint8Array[] = [];
  let size = 0;
  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    size += value.byteLength;
    if (size > MAX_PROVIDER_RESPONSE_BYTES) {
      await reader.cancel();
      throw new Error();
    }
    chunks.push(value);
  }
  const bytes = new Uint8Array(size);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.byteLength;
  }
  return JSON.parse(new TextDecoder().decode(bytes)) as unknown;
}

function providerErrors(value: unknown): ProviderError[] | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) return undefined;
  const errors = (value as Record<string, unknown>).errors;
  if (!Array.isArray(errors)) return undefined;
  const valid: ProviderError[] = [];
  for (const item of errors.slice(0, MAX_PROVIDER_ERRORS)) {
    if (!item || typeof item !== "object" || Array.isArray(item)) continue;
    const { code, message } = item as Record<string, unknown>;
    if (
      !(
        (typeof code === "string" && code.length <= MAX_PROVIDER_CODE_LENGTH) ||
        (typeof code === "number" && Number.isSafeInteger(code))
      ) ||
      typeof message !== "string" ||
      message.length > MAX_PROVIDER_MESSAGE_LENGTH
    ) {
      continue;
    }
    valid.push({ code, message });
  }
  return valid.length ? valid : undefined;
}

export async function readDnsProviderResponse(
  response: Response,
  operation: DnsOperation,
): Promise<Record<string, unknown>> {
  let body: unknown;
  try {
    body = await boundedProviderJson(response);
  } catch {
    throw new DnsProviderError(operation, "invalid_provider_response", response.status);
  }
  const errors = providerErrors(body);
  if (!response.ok) {
    throw new DnsProviderError(operation, "provider_http_error", response.status, errors);
  }
  if (!body || typeof body !== "object" || Array.isArray(body)) {
    throw new DnsProviderError(operation, "invalid_provider_response", response.status);
  }
  const record = body as Record<string, unknown>;
  if (record.success !== true) {
    throw new DnsProviderError(operation, "provider_rejected", response.status, errors);
  }
  return record;
}
