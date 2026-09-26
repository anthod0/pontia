import { redirect } from '@sveltejs/kit';
import { eq } from 'drizzle-orm';
import { currentLogin, environment } from '$lib/server/auth/http';
import { database } from '$lib/server/db';
import { accounts } from '$lib/server/db/schema';
import type { PageServerLoad } from './$types';

export const load: PageServerLoad = async (event) => {
	event.setHeaders({ 'cache-control': 'private, no-store' });
	const user = await currentLogin(event);
	if (!user) redirect(303, '/login');
	const linked = await database(environment(event).DB)
		.select({
			provider: accounts.provider,
			email: accounts.email,
			emailVerified: accounts.emailVerified
		})
		.from(accounts)
		.where(eq(accounts.userId, user.user_id));
	return { user, accounts: linked, error: event.url.searchParams.get('error') };
};
