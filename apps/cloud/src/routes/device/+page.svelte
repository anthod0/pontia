<script lang="ts">
	import AuthProviderIcon from '$lib/components/AuthProviderIcon.svelte';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import AuthLayout from '$lib/components/AuthLayout.svelte';
	import { m } from '$lib/paraglide/messages';
	import { getLocale, localizeHref } from '$lib/paraglide/runtime';
	import type { ActionData, PageData } from './$types';

	let { data, form }: { data: PageData; form: ActionData } = $props();
	let userCode = $derived(data.userCode);
	const returnTo = $derived(
		localizeHref(`/device${data.userCode ? `?user_code=${encodeURIComponent(data.userCode)}` : ''}`)
	);
	const resultMessage = $derived(
		data.result === 'approved'
			? m.device_approved()
			: data.result === 'denied'
				? m.device_denied()
				: data.result === 'invalid'
					? m.device_invalid()
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
	><title>{m.device_title()}</title><meta
		name="robots"
		content="noindex"
	/></svelte:head
>

<AuthLayout>
	<h1>{m.device_heading()}</h1>
	{#if resultMessage}
		<p class:auth-error={data.result === 'invalid'} role="status">
			{resultMessage}
		</p>
	{:else if !data.user}
		<p>{m.device_sign_in_prompt()}</p>
		{#if data.userCode}<p class="device-code">{data.userCode}</p>{/if}
		<div class="providers">
			<form
				method="POST"
				action={`/api/auth/google/login?locale=${getLocale()}&return_to=${encodeURIComponent(returnTo)}`}
			>
				<Button type="submit" class="button button-secondary"
					><AuthProviderIcon provider="google" />{m.continue_with_google()}</Button
				>
			</form>
			<form
				method="POST"
				action={`/api/auth/github/login?locale=${getLocale()}&return_to=${encodeURIComponent(returnTo)}`}
			>
				<Button type="submit" class="button button-secondary"
					><AuthProviderIcon provider="github" />{m.continue_with_github()}</Button
				>
			</form>
		</div>
	{:else}
		<p>{m.device_compare_code()}</p>
		<form method="POST" class="code-entry">
			<Input
				id="user_code"
				name="user_code"
				aria-label={m.device_user_code()}
				class="user-code-input"
				value={userCode}
				oninput={updateUserCode}
				placeholder="XXXX-XXXX"
				autocomplete="one-time-code"
				autocapitalize="characters"
				maxlength={9}
				pattern={'[BCDFGHJKLMNPQRSTVWXZ]{4}-[BCDFGHJKLMNPQRSTVWXZ]{4}'}
				required
			/>
			{#if form?.error}<p class="auth-error" role="alert">{form.error}</p>{/if}
			<div class="decisions">
				<Button type="submit" formaction="?/approve" class="button approve"
					>{m.device_approve()}</Button
				>
				<Button
					type="submit"
					formaction="?/deny"
					class="button button-secondary">{m.device_deny()}</Button
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
		border-radius: 4px;
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
		border-radius: 4px;
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
	/* Approve stays first in the DOM so Enter submits it; show it on the right. */
	.decisions :global(.approve) {
		order: 1;
	}
</style>
