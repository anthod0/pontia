<script lang="ts">
	import './layout.css';
	import { applyTheme, resolvedTheme } from '$lib/theme';

	let { children } = $props();

	$effect(() => applyTheme($resolvedTheme));
</script>

<svelte:head>
	<link rel="icon" type="image/svg+xml" href="/logo.svg" />
</svelte:head>

{@render children()}
