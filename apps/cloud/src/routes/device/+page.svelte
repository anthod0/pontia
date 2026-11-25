<script lang="ts">
	import AuthProviderIcon from '$lib/components/AuthProviderIcon.svelte';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import AuthLayout from '$lib/components/AuthLayout.svelte';
	import { m } from '$lib/paraglide/messages';
	import { getLocale } from '$lib/paraglide/runtime';
	import type { ActionData, PageData } from './$types';

	let { data, form }: { data: PageData; form: ActionData } = $props();
	let userCode = $derived(data.userCode);
	let timedOut = $state(false);
	const registration = $derived(data.registration);

	$effect(() => {
		if (!registration) return;
		const params = new URLSearchParams({
			init_device: registration.id,
			approved_at: String(registration.approvedAt)
		});
		const controller = new AbortController();
		let timer: ReturnType<typeof setTimeout>;
		timedOut = registration.remainingMs <= 0;
		if (registration.remainingMs <= 0) return;
		const deadline = setTimeout(() => {
			timedOut = true;
			controller.abort();
			clearTimeout(timer);
		}, registration.remainingMs);

		async function poll() {
			try {
				const response = await fetch(`/api/device/registration?${params}`, { signal: controller.signal });
				if (response.ok) {
					const { handle } = await response.json() as { handle: string | null };
					if (handle && !controller.signal.aborted) {
						window.location.replace(`${data.dashboard}/${encodeURIComponent(handle)}`);
						return;
					}
				}
			} catch {
				// A transient connection failure can be retried until the deadline.
			}
			if (!controller.signal.aborted) timer = setTimeout(poll, 2500);
		}
		timer = setTimeout(poll, 2500);
		return () => {
			controller.abort();
			clearTimeout(timer);
			clearTimeout(deadline);
		};
	});
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
	<h1>{registration ? m.device_authorized_heading() : m.device_heading()}</h1>
	{#if registration}
		<p role="status">{timedOut || !registration.remainingMs ? m.device_wait_timeout() : m.device_wait_registration()}</p>
		<div class="dashboard-link">
			<Button href={data.dashboard} class="button button-secondary">{m.device_open_dashboard()}</Button>
		</div>
	{:else if resultMessage}
		<p class:auth-error={data.result === 'invalid'} role="status">
			{resultMessage}
		</p>
	{:else if !data.user}
		<p>{m.device_sign_in_prompt()}</p>
		{#if data.userCode}<p class="device-code">{data.userCode}</p>{/if}
		<div class="providers">
			<form
				method="POST"
				action={`/api/auth/google/login?locale=${getLocale()}&return_to=${encodeURIComponent(data.returnTo)}`}
			>
				<Button type="submit" class="button button-secondary"
					><AuthProviderIcon provider="google" />{m.continue_with_google()}</Button
				>
			</form>
			<form
				method="POST"
				action={`/api/auth/github/login?locale=${getLocale()}&return_to=${encodeURIComponent(data.returnTo)}`}
			>
				<Button type="submit" class="button button-secondary"
					><AuthProviderIcon provider="github" />{m.continue_with_github()}</Button
				>
			</form>
		</div>
	{:else}
		<p>{m.device_compare_code()}</p>
		<form method="POST" class="code-entry">
			{#if data.initDevice && userCode && userCode === data.userCode}
				<input type="hidden" name="init_device" value={data.initDevice} />
			{/if}
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
	.dashboard-link,
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
