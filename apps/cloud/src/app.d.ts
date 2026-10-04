/// <reference path="../worker-configuration.d.ts" />

// See https://svelte.dev/docs/kit/types#app.d.ts
// for information about these interfaces
declare global {
  namespace App {
    interface Platform {
      env: Env & {
        AUTH_ORIGIN: string;
        GOOGLE_CLIENT_ID: string;
        GOOGLE_CLIENT_SECRET: string;
        GITHUB_CLIENT_ID: string;
        GITHUB_CLIENT_SECRET: string;
        JWT_SECRET: string;
        OAUTH_COOKIE_SECRET: string;
        E2E_CAPABILITY_SIGNING_KEY: string;
        E2E_CAPABILITY_VERIFICATION_KEY: string;
        E2E_REGISTRATION_PROOF_PRIVATE_KEY: string;
        E2E_REGISTRATION_PROOF_PUBLIC_KEY: string;
        CLOUDFLARE_DNS_TOKEN: string;
        CLOUDFLARE_DNS_ZONE_ID: string;
        EDGE_NETWORK_RATE_LIMIT: RateLimit;
        EDGE_DNS_RATE_LIMIT: RateLimit;
      };
      ctx: ExecutionContext;
      caches: CacheStorage;
      cf?: IncomingRequestCfProperties;
    }

    // interface Error {}
    // interface Locals {}
    // interface PageData {}
    // interface PageState {}
  }
}

export {};
