import { DnsProviderError, readProviderResponse } from "./dns-errors";

export class CloudflareDnsProvider {
  constructor(
    private readonly token: string,
    private readonly zoneId: string,
    private readonly previewZoneId: string,
  ) {}

  private baseUrl(zoneId: string) {
    return `https://api.cloudflare.com./client/v4/zones/${encodeURIComponent(zoneId)}/dns_records`;
  }

  private zoneFor(hostname: string) {
    return hostname.endsWith(".pontia.run") ? this.previewZoneId : this.zoneId;
  }

  private headers() {
    return { Authorization: `Bearer ${this.token}`, "Content-Type": "application/json" };
  }

  async cleanupExpiredTxt(now: Date) {
    let deleted = 0;
    for (const zoneId of [this.zoneId, this.previewZoneId]) {
      deleted += await this.cleanupExpiredZoneTxt(now, zoneId);
    }
    return deleted;
  }

  private async cleanupExpiredZoneTxt(now: Date, zoneId: string) {
    const expiredIds: string[] = [];
    // Enumerate before deleting so pagination cannot skip records after a deletion.
    for (let page = 1; ; page++) {
      const response = await fetch(`${this.baseUrl(zoneId)}?type=TXT&per_page=20&page=${page}`, {
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
          /^_acme-challenge\.([a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?)\.(?:edge\.pontia\.dev|pontia\.run)$/.exec(
            record.name,
          );
        const created = Date.parse(record.created_on);
        if (
          match &&
          this.zoneFor(record.name) === zoneId &&
          Number.isFinite(created) &&
          now.getTime() - created > 24 * 60 * 60 * 1000
        )
          expiredIds.push(record.id);
      }
      if (body.result.length < 20) break;
    }
    for (const id of expiredIds) {
      const response = await fetch(`${this.baseUrl(zoneId)}/${encodeURIComponent(id)}`, {
        method: "DELETE",
        headers: this.headers(),
        redirect: "manual",
      });
      if (response.status !== 404) await readProviderResponse(response, "delete");
    }
    return expiredIds.length;
  }
}
