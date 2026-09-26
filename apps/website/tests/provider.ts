import { spyOn } from 'bun:test';

type FetchRequest = (
	...args: Parameters<typeof fetch>
) => ReturnType<typeof fetch>;

export function mockProvider(implementation: FetchRequest) {
	return spyOn(
		globalThis as { fetch: FetchRequest },
		'fetch'
	).mockImplementation(implementation);
}
