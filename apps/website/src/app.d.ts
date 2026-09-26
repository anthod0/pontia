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
