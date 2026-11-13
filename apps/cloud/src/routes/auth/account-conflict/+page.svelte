<script lang="ts">
	import { Button } from '$lib/components/ui/button';
	import AuthLayout from '$lib/components/AuthLayout.svelte';
	import { ParaglideMessage } from '@inlang/paraglide-js-svelte';
	import { m } from '$lib/paraglide/messages';
	import { getLocale } from '$lib/paraglide/runtime';
	import type { PageData } from './$types';
	let { data }: { data: PageData } = $props();
	const providerName = $derived(
		data.provider === 'google' ? 'Google' : 'GitHub'
	);
</script>

<svelte:head
	><title>{m.conflict_title()}</title><meta
		name="robots"
		content="noindex"
	/></svelte:head
>

<AuthLayout>
	<h1>{m.conflict_heading()}</h1>
	<p>
		<ParaglideMessage message={m.conflict_text} inputs={{ email: data.email, provider: providerName }}>
			{#snippet email({ children })}<strong>{@render children?.()}</strong>{/snippet}
		</ParaglideMessage>
	</p>
	<div class="choices">
		<form method="POST" action={`/api/auth/pending/bind?locale=${getLocale()}`}>
			<Button type="submit" class="button"
				>{m.conflict_connect()}</Button
			>
		</form>
		<form method="POST" action={`/api/auth/pending/create?locale=${getLocale()}`}>
			<Button type="submit" class="button button-secondary"
				>{m.conflict_create()}</Button
			>
		</form>
	</div>
	<p class="hint">{m.conflict_expires()}</p>
</AuthLayout>

<style>
	.choices {
		margin-block: 28px;
	}
	.hint {
		font-size: 12px !important;
	}
</style>
