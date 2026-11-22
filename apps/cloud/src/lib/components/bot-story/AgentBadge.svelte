<script lang="ts">
	import PlugsConnectedIcon from 'phosphor-svelte/lib/PlugsConnectedIcon';
	import type { Agent } from '$lib/bot-stories';
	let { agent }: { agent: Agent } = $props();
</script>

<span class="badge">
	{#if agent.logo}
		<img class:light-logo={agent.darkLogo} src={agent.logo} alt="" width="14" height="14" loading="lazy" />
		{#if agent.darkLogo}<img class="dark-logo" src={agent.darkLogo} alt="" width="14" height="14" loading="lazy" />{/if}
	{:else}
		<PlugsConnectedIcon size={14} />
	{/if}
	{agent.client}
</span>

<style>
	.badge { display: inline-flex; align-items: center; gap: 7px; font: 11px var(--font-mono); color: var(--muted-foreground); white-space: nowrap; }
	img { width: 14px; height: 14px; object-fit: contain; }
	.dark-logo { display: none; }
	:global(.dark) .light-logo { display: none; }
	:global(.dark) .dark-logo { display: block; }
</style>
