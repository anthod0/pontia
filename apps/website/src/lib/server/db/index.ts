import { drizzle } from 'drizzle-orm/d1';

export function database(binding: D1Database) {
	return drizzle(binding.withSession('first-primary'));
}

export type Database = ReturnType<typeof database>;
