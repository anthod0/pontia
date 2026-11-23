<script lang="ts">
	import { inlineTokens } from './text';

	/** One line of mock Markdown: **bold**, `code`, and [file](path) links that call `onlink`. */
	let { text, code = 'bg-muted', onlink }: { text: string; code?: string; onlink?: (path: string) => void } = $props();
</script>

{#each inlineTokens(text) as token}
	{#if token.kind === 'strong'}<strong class="font-semibold text-heading">{token.value}</strong>
	{:else if token.kind === 'code'}<code class="rounded-sm px-1 py-0.5 font-mono text-xs text-primary {code}">{token.value}</code>
	{:else if token.kind === 'link' && onlink}
		{@const path = token.path}
		<button type="button" class="rounded-sm text-primary underline underline-offset-2 hover:text-primary-solid-hover focus-visible:ring-2 focus-visible:ring-primary" title={path} onclick={() => onlink(path)}>{token.value}</button>
	{:else}{token.value}{/if}
{/each}
