import type { Handle } from '@sveltejs/kit';

const privatePages = ['/login', '/account'];

export const handle: Handle = async ({ event, resolve }) => {
	const response = await resolve(event);
	const { pathname } = event.url;
	if (pathname.startsWith('/api/') || privatePages.includes(pathname)) {
		response.headers.set('cache-control', 'private, no-store');
	}
	return response;
};
