import type { DnsARecord, DnsProvider } from "./cleanup";
import { DnsProviderError, readProviderResponse } from "./dns-errors";

export class CloudflareDnsProvider implements DnsProvider {
  constructor(
    private readonly token: string,
    private readonly zoneId: string,
  ) {}

  private baseUrl() {
    return `https://api.cloudflare.com./client/v4/zones/${encodeURIComponent(this.zoneId)}/dns_records`;
  }

  private headers() {
    return { Authorization: `Bearer ${this.token}`, "Content-Type": "application/json" };
  }

  async findA(hostname: string): Promise<DnsARecord | null> {
    let response: Response;
    try {
      response = await fetch(`${this.baseUrl()}?type=A&name=${encodeURIComponent(hostname)}`, {
        method: "GET",
        headers: this.headers(),
        redirect: "manual",
      });
    } catch {
      throw new DnsProviderError("lookup", "network_error");
    }
    const body = await readProviderResponse(response, "lookup");
    if (!Array.isArray(body.result)) {
      throw new DnsProviderError("lookup", "invalid_provider_response", response.status);
    }
    if (body.result.length > 1) {
      throw new DnsProviderError("lookup", "local_validation_error", response.status);
    }
    const record = body.result[0] as Record<string, unknown> | undefined;
    if (record === undefined) return null;
    if (
      record.type !== "A" ||
      record.name !== hostname ||
      typeof record.id !== "string" ||
      record.id.length === 0
    ) {
      throw new DnsProviderError("lookup", "local_validation_error", response.status);
    }
    return { id: record.id, hostname };
  }

  async deleteA(record: DnsARecord) {
    let response: Response;
    try {
      response = await fetch(`${this.baseUrl()}/${encodeURIComponent(record.id)}`, {
        method: "DELETE",
        headers: this.headers(),
        redirect: "manual",
      });
    } catch {
      throw new DnsProviderError("delete", "network_error");
    }
    if (response.status === 404) {
      let deletionError: DnsProviderError;
      try {
        await readProviderResponse(response, "delete");
        deletionError = new DnsProviderError("delete", "provider_http_error", 404);
      } catch (error) {
        deletionError =
          error instanceof DnsProviderError
            ? error
            : new DnsProviderError("delete", "provider_http_error", 404);
      }
      if ((await this.findA(record.hostname)) === null) return;
      throw deletionError;
    }
    await readProviderResponse(response, "delete");
  }
}
