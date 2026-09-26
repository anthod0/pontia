<script lang="ts">
	import GithubLogoIcon from 'phosphor-svelte/lib/GithubLogoIcon';
	import GoogleLogoIcon from 'phosphor-svelte/lib/GoogleLogoIcon';
	import { Button } from '$lib/components/ui/button';
	import AuthLayout from '$lib/components/AuthLayout.svelte';
	import { authError } from '$lib/auth-errors';
	import type { PageData } from './$types';
	let { data }: { data: PageData } = $props();
	const message = $derived(authError(data.error));
</script>

<svelte:head
	><title>Sign in — Pontia</title><meta
		name="robots"
		content="noindex"
	/></svelte:head
>

<AuthLayout>
	<h1>Welcome to Pontia.</h1>
	<p>Sign in or create your account to get started.</p>
	{#if message}<p class="auth-error" role="alert">{message}</p>{/if}
	<div class="providers">
		<form method="POST" action="/api/auth/google/login">
			<Button type="submit" class="button button-secondary"
				><GoogleLogoIcon size={20} />Continue with Google</Button
			>
		</form>
		<form method="POST" action="/api/auth/github/login">
			<Button type="submit" class="button button-secondary"
				><GithubLogoIcon size={20} />Continue with GitHub</Button
			>
		</form>
	</div>
	<p class="hint">
		New here? Your account is created when you first sign in. To use both
		providers with one Pontia account, sign in first and link the other in your
		account settings.
	</p>
	{#if data.hasCredential}
		<form method="POST" action="/api/auth/logout">
			<Button type="submit" class="button button-secondary">Sign out</Button>
		</form>
	{/if}
</AuthLayout>

<style>
	.providers {
		margin-block: 28px;
	}
	.hint {
		font-size: 12px !important;
	}
</style>
