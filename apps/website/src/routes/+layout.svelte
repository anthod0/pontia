<script lang="ts">
	import './layout.css';

	let { children } = $props();
</script>

<svelte:head>
	<link rel="icon" type="image/svg+xml" href="/logo.svg" />
	<meta name="theme-color" content="#6b48fe" />
</svelte:head>

{@render children()}
