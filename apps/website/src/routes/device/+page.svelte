<script lang="ts">
	import GithubLogoIcon from 'phosphor-svelte/lib/GithubLogoIcon';
	import GoogleLogoIcon from 'phosphor-svelte/lib/GoogleLogoIcon';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import AuthLayout from '$lib/components/AuthLayout.svelte';
	import type { ActionData, PageData } from './$types';

	let { data, form }: { data: PageData; form: ActionData } = $props();
	const returnTo = $derived(
		`/device${data.userCode ? `?user_code=${encodeURIComponent(data.userCode)}` : ''}`
	);
	const resultMessage = $derived(
		data.result === 'approved'
			? 'Access approved. You can return to the terminal.'
			: data.result === 'denied'
				? 'Access denied. You can return to the terminal.'
				: data.result === 'invalid'
					? 'This code is invalid, expired, or has already been used.'
					: null
	);
</script>

<svelte:head
	><title>Confirm device — Pontia</title><meta
		name="robots"
		content="noindex"
	/></svelte:head
>

<AuthLayout>
	<h1>Confirm device access.</h1>
	{#if resultMessage}
		<p class:auth-error={data.result === 'invalid'} role="status">
			{resultMessage}
		</p>
	{:else if !data.user}
		<p>Sign in to approve or deny the request from your terminal.</p>
		{#if data.userCode}<p class="device-code">{data.userCode}</p>{/if}
		<div class="providers">
			<form
				method="POST"
				action={`/api/auth/google/login?return_to=${encodeURIComponent(returnTo)}`}
			>
				<Button type="submit" class="button button-secondary"
					><GoogleLogoIcon size={20} />Continue with Google</Button
				>
			</form>
			<form
				method="POST"
				action={`/api/auth/github/login?return_to=${encodeURIComponent(returnTo)}`}
			>
				<Button type="submit" class="button button-secondary"
					><GithubLogoIcon size={20} />Continue with GitHub</Button
				>
			</form>
		</div>
	{:else}
		<p>Compare the code below with the code shown in your terminal.</p>
		<form method="GET" class="code-entry">
			<label for="user_code">User code</label>
			<Input
				id="user_code"
				name="user_code"
				class="user-code-input"
				value={data.userCode}
				placeholder="XXXX-XXXX"
				autocomplete="one-time-code"
				required
			/>
			<Button type="submit" class="button button-secondary">Check code</Button>
		</form>
		{#if data.userCode}
			<p class="device-code">{data.userCode}</p>
			{#if form?.error}<p class="auth-error" role="alert">{form.error}</p>{/if}
			<div class="decisions">
				<form method="POST" action="?/approve">
					<input type="hidden" name="user_code" value={data.userCode} />
					<Button type="submit" class="button">Approve</Button>
				</form>
				<form method="POST" action="?/deny">
					<input type="hidden" name="user_code" value={data.userCode} />
					<Button type="submit" class="button button-secondary">Deny</Button>
				</form>
			</div>
		{/if}
	{/if}
</AuthLayout>

<style>
	.providers,
	.code-entry,
	.decisions {
		margin-top: 28px;
	}
	.code-entry label {
		display: block;
		margin-bottom: 8px;
		font-size: 12px;
		font-weight: 500;
	}
	.code-entry :global(.user-code-input) {
		width: 100%;
		padding: 13px;
		border: 1px solid var(--border);
		background: var(--background);
		font: 20px var(--font-mono);
		letter-spacing: 0.12em;
		text-transform: uppercase;
	}
	.device-code {
		margin-top: 28px;
		padding: 20px;
		border: 1px solid var(--border);
		color: var(--heading) !important;
		font: 28px var(--font-mono) !important;
		letter-spacing: 0.12em;
		text-align: center;
	}
	.decisions {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 12px;
	}
	.decisions form {
		margin-top: 0;
	}
</style>
