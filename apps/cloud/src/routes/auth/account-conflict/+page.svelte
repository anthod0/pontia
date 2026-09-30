<script lang="ts">
	import { Button } from '$lib/components/ui/button';
	import AuthLayout from '$lib/components/AuthLayout.svelte';
	import type { PageData } from './$types';
	let { data }: { data: PageData } = $props();
	const providerName = $derived(
		data.provider === 'google' ? 'Google' : 'GitHub'
	);
</script>

<svelte:head
	><title>Choose your account — Pontia</title><meta
		name="robots"
		content="noindex"
	/></svelte:head
>

<AuthLayout>
	<h1>This email is already registered.</h1>
	<p>
		<strong>{data.email}</strong> is already associated with a verified
		{providerName} account. Verify that account to use both sign-in methods with the
		same Pontia account, or create a separate account.
	</p>
	<div class="choices">
		<form method="POST" action="/api/auth/pending/bind">
			<Button type="submit" class="button"
				>Connect to existing account</Button
			>
		</form>
		<form method="POST" action="/api/auth/pending/create">
			<Button type="submit" class="button button-secondary"
				>Create a separate account</Button
			>
		</form>
	</div>
	<p class="hint">This choice expires in five minutes.</p>
</AuthLayout>

<style>
	.choices {
		margin-block: 28px;
	}
	.hint {
		font-size: 12px !important;
	}
</style>
