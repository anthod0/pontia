const messages: Record<string, string> = {
	invalid_credentials: 'Please sign in again to continue.',
	invalid_oauth: 'Sign-in was cancelled or expired. Please try again.',
	account_conflict:
		'This account is already linked, or you already have an account with this provider.',
	provider_failed:
		'We could not complete sign-in with this provider. Please try again.'
};

export function authError(code: string | null) {
	return code
		? (messages[code] ?? 'Sign-in could not be completed. Please try again.')
		: null;
}
