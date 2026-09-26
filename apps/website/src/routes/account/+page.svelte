<script lang="ts">
	import { Button } from '$lib/components/ui/button';
	import AuthLayout from '$lib/components/AuthLayout.svelte';
	import { authError } from '$lib/auth-errors';
	import type { PageData } from './$types';
	let { data }: { data: PageData } = $props();
	const message = $derived(authError(data.error));
	const providers = [
		{ id: 'google', name: 'Google' },
		{ id: 'github', name: 'GitHub' }
	];
</script>

<svelte:head
	><title>Your account — Pontia</title><meta
		name="robots"
		content="noindex"
	/></svelte:head
>

<AuthLayout>
	<h1>{data.user.display_name ?? 'Your account'}</h1>
	<p>Manage the accounts you use to sign in to Pontia.</p>
	{#if message}<p class="auth-error" role="alert">{message}</p>{/if}
	<ul>
		{#each providers as provider}
			{@const account = data.accounts.find(
				(item) => item.provider === provider.id
			)}
			<li>
				<h2>{provider.name}</h2>
				{#if account}
					<p>Linked</p>
					{#if account.email}<p class="email">
							{account.email} · {account.emailVerified
								? 'Verified email'
								: 'Unverified email'}
						</p>{/if}
				{:else}
					<form method="POST" action={`/api/auth/${provider.id}/bind`}>
						<Button type="submit" class="button button-secondary"
							>Link {provider.name}</Button
						>
					</form>
				{/if}
			</li>
		{/each}
	</ul>
	<form method="POST" action="/api/auth/logout">
		<Button type="submit" class="button">Sign out</Button>
	</form>
</AuthLayout>

<style>
	ul {
		list-style: none;
		margin: 28px 0;
		padding: 0;
	}
	li {
		border-top: 1px solid var(--border);
		padding-block: 20px;
	}
	li:last-child {
		border-bottom: 1px solid var(--border);
	}
	h2 {
		font-size: 18px;
		margin-bottom: 8px;
		letter-spacing: -0.02em;
	}
	.email {
		overflow-wrap: anywhere;
		font-size: 12px !important;
	}
</style>
