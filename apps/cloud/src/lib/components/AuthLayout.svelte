<script lang="ts">
	import type { Snippet } from 'svelte';
	let { children }: { children: Snippet } = $props();
</script>

<header class="site-header page-width">
	<a class="brand" href="/" aria-label="Pontia home"
		><img src="/logo.svg" alt="" /><span>Pontia</span></a
	>
	<a class="nav-link" href="/">Back to home</a>
</header>
<main class="auth-content">{@render children()}</main>

<style>
	.auth-content {
		width: min(440px, calc(100% - 40px));
		margin: 72px auto;
	}
	.auth-content :global(h1) {
		font-size: 32px;
		font-weight: 500;
		letter-spacing: -0.04em;
		margin-bottom: 16px;
	}
	.auth-content :global(p) {
		color: var(--muted-foreground);
		line-height: 1.7;
		font-size: 14px;
	}
	.auth-content :global(form) {
		margin-top: 14px;
	}
	.auth-content :global(.button) {
		width: 100%;
	}
	.auth-content :global(.auth-error) {
		color: #9d3229;
		border: 1px solid #dab0aa;
		padding: 12px;
		margin-block: 20px;
	}
	@media (max-width: 600px) {
		.auth-content {
			margin-block: 44px;
		}
	}
</style>
