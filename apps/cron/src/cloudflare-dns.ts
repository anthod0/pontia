import type { DnsARecord, DnsProvider } from "./cleanup";

type LookupResponse = {
  success?: unknown;
  result?: Array<{ id?: unknown; name?: unknown; type?: unknown }>;
};

type Fetcher = (input: string, init: RequestInit) => Promise<Response>;

export class CloudflareDnsProvider implements DnsProvider {
  constructor(
    private readonly token: string,
    private readonly zoneId: string,
    private readonly fetcher: Fetcher = fetch,
  ) {}

  private baseUrl() {
    return `https://api.cloudflare.com/client/v4/zones/${encodeURIComponent(this.zoneId)}/dns_records`;
  }

  private headers() {
    return { Authorization: `Bearer ${this.token}`, "Content-Type": "application/json" };
  }

  async findA(hostname: string): Promise<DnsARecord | null> {
    let response: Response;
    try {
      response = await this.fetcher(
        `${this.baseUrl()}?type=A&name=${encodeURIComponent(hostname)}`,
        { method: "GET", headers: this.headers(), redirect: "manual" },
      );
    } catch {
      throw new Error("dns_lookup_failed");
    }
    if (!response.ok) throw new Error("dns_lookup_failed");

    let body: LookupResponse;
    try {
      body = (await response.json()) as LookupResponse;
    } catch {
      throw new Error("dns_lookup_invalid_response");
    }
    if (body.success !== true || !Array.isArray(body.result)) {
      throw new Error("dns_lookup_invalid_response");
    }
    if (body.result.length > 1) throw new Error("dns_lookup_duplicate_records");
    const record = body.result[0];
    if (record === undefined) return null;
    if (
      record.type !== "A" ||
      record.name !== hostname ||
      typeof record.id !== "string" ||
      record.id.length === 0
    ) {
      throw new Error("dns_lookup_unexpected_record");
    }
    return { id: record.id, hostname };
  }

  async deleteA(record: DnsARecord) {
    let response: Response;
    try {
      response = await this.fetcher(`${this.baseUrl()}/${encodeURIComponent(record.id)}`, {
        method: "DELETE",
        headers: this.headers(),
        redirect: "manual",
      });
    } catch {
      throw new Error("dns_delete_failed");
    }
    if (response.status === 404) {
      if ((await this.findA(record.hostname)) === null) return;
      throw new Error("dns_delete_not_confirmed");
    }
    if (!response.ok) throw new Error("dns_delete_failed");

    let body: { success?: unknown };
    try {
      body = (await response.json()) as { success?: unknown };
    } catch {
      throw new Error("dns_delete_invalid_response");
    }
    if (body.success !== true) throw new Error("dns_delete_rejected");
  }
}
