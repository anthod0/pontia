<script lang="ts">
	import GithubLogoIcon from 'phosphor-svelte/lib/GithubLogoIcon';
	import GoogleLogoIcon from 'phosphor-svelte/lib/GoogleLogoIcon';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import AuthLayout from '$lib/components/AuthLayout.svelte';
	import type { ActionData, PageData } from './$types';

	let { data, form }: { data: PageData; form: ActionData } = $props();
	let userCode = $derived(data.userCode);
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

	function formatUserCode(value: string) {
		const normalized = value
			.toUpperCase()
			.replace(/[^BCDFGHJKLMNPQRSTVWXZ]/g, '')
			.slice(0, 8);
		return normalized.length > 4
			? `${normalized.slice(0, 4)}-${normalized.slice(4)}`
			: normalized;
	}

	function updateUserCode(event: Event) {
		userCode = formatUserCode((event.currentTarget as HTMLInputElement).value);
	}
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
		<p>Compare this code with the code shown in your terminal.</p>
		<form method="POST" class="code-entry">
			<Input
				id="user_code"
				name="user_code"
				aria-label="User code"
				class="user-code-input"
				value={userCode}
				oninput={updateUserCode}
				placeholder="XXXX-XXXX"
				autocomplete="one-time-code"
				autocapitalize="characters"
				maxlength={9}
				pattern="[BCDFGHJKLMNPQRSTVWXZ]{4}-[BCDFGHJKLMNPQRSTVWXZ]{4}"
				required
			/>
			{#if form?.error}<p class="auth-error" role="alert">{form.error}</p>{/if}
			<div class="decisions">
				<Button type="submit" formaction="?/approve" class="button"
					>Approve</Button
				>
				<Button
					type="submit"
					formaction="?/deny"
					class="button button-secondary">Deny</Button
				>
			</div>
		</form>
	{/if}
</AuthLayout>

<style>
	.providers,
	.code-entry,
	.decisions {
		margin-top: 28px;
	}
	.code-entry :global(.user-code-input) {
		width: 100%;
		padding: 22px;
		border: 1px solid var(--border);
		background: var(--background);
		font: 24px var(--font-mono);
		letter-spacing: 0.12em;
		text-align: center;
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
</style>
