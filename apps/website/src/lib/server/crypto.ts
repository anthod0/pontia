import { base64url } from 'jose';

export async function sha256Base64url(value: string) {
	return base64url.encode(
		new Uint8Array(
			await crypto.subtle.digest('SHA-256', new TextEncoder().encode(value))
		)
	);
}
