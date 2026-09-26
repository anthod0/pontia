export type Provider = 'google' | 'github';

export interface AccountProfile {
	provider: Provider;
	providerSubject: string;
	email: string | null;
	emailVerified: boolean;
	displayName: string | null;
	avatarUrl: string | null;
}

export class AuthError extends Error {
	constructor(
		public readonly code:
			| 'invalid_credentials'
			| 'invalid_oauth'
			| 'account_conflict'
			| 'provider_failed'
	) {
		super(code);
	}
}
