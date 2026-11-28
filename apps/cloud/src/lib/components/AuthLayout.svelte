<script lang="ts">
	import SiteHeader from '$lib/components/SiteHeader.svelte';
	import type { Snippet } from 'svelte';
	let { children, wide = false }: { children: Snippet; wide?: boolean } = $props();
</script>

<SiteHeader />
<main class:wide class="auth-content">{@render children()}</main>

<style>
	.auth-content {
		width: min(440px, calc(100% - 40px));
		margin: 72px auto;
	}
	.auth-content.wide {
		width: min(800px, calc(100% - 40px));
	}
	.auth-content :global(h1) {
		font-size: var(--text-28);
		font-weight: 600;
		letter-spacing: -0.025em;
		margin-bottom: 16px;
	}
	.auth-content :global(:where(p)) {
		color: var(--muted-foreground);
		line-height: 1.6;
		font-size: 14px;
	}
	.auth-content :global(form) {
		margin-top: 14px;
	}
	@media (max-width: 600px) {
		.auth-content {
			margin-block: 44px;
		}
	}
</style>
