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

  async cleanupExpiredTxt(now: Date) {
    const expiredIds: string[] = [];
    // Enumerate before deleting so pagination cannot skip records after a deletion.
    for (let page = 1; ; page++) {
      const response = await fetch(`${this.baseUrl()}?type=TXT&per_page=20&page=${page}`, {
        headers: this.headers(),
        redirect: "manual",
      });
      const body = await readProviderResponse(response, "lookup");
      if (!Array.isArray(body.result))
        throw new DnsProviderError("lookup", "invalid_provider_response");
      for (const record of body.result as Record<string, unknown>[]) {
        if (
          record.type !== "TXT" ||
          typeof record.name !== "string" ||
          typeof record.id !== "string" ||
          typeof record.created_on !== "string"
        )
          continue;
        const match =
          /^_acme-challenge\.([a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?)\.edge\.pontia\.dev$/.exec(
            record.name,
          );
        const created = Date.parse(record.created_on);
        if (match && Number.isFinite(created) && now.getTime() - created > 24 * 60 * 60 * 1000)
          expiredIds.push(record.id);
      }
      if (body.result.length < 20) break;
    }
    for (const id of expiredIds) {
      const response = await fetch(`${this.baseUrl()}/${encodeURIComponent(id)}`, {
        method: "DELETE",
        headers: this.headers(),
        redirect: "manual",
      });
      if (response.status !== 404) await readProviderResponse(response, "delete");
    }
    return expiredIds.length;
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
