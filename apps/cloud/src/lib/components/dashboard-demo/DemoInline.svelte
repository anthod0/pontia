<script lang="ts">
	import ArrowSquareOutIcon from 'phosphor-svelte/lib/ArrowSquareOutIcon';
	import FileTextIcon from 'phosphor-svelte/lib/FileTextIcon';
	import { inlineTokens } from './text';

	/** One line of mock Markdown: **bold**, `code`, [file](path) links that call `onlink`, and localhost links that call `onpreview`. */
	let { text, code = 'bg-muted', onlink, onpreview }: { text: string; code?: string; onlink?: (path: string) => void; onpreview?: (url: string) => void } = $props();
	const link = 'inline rounded-sm text-left align-baseline text-primary underline underline-offset-2 hover:text-primary-solid-hover focus-visible:ring-2 focus-visible:ring-primary';
	const icon = 'ml-1 inline-block size-3.5 align-[-0.125em]';
</script>

{#each inlineTokens(text) as token}
	{#if token.kind === 'strong'}<strong class="font-semibold text-heading">{token.value}</strong>
	{:else if token.kind === 'code'}<code class="rounded-sm px-1 py-0.5 font-mono text-xs text-primary {code}">{token.value}</code>
	{:else if token.kind === 'link' && onlink}
		{@const path = token.path}
		<button type="button" class={link} title={path} aria-label={`Preview ${token.value}`} onclick={() => onlink(path)}>{token.value}<FileTextIcon class={icon} aria-hidden="true" /></button>
	{:else if token.kind === 'preview' && onpreview}
		{@const url = token.url}
		<a href={url} class={link} title={url} onclick={(event) => { event.preventDefault(); onpreview(url); }}>{token.value}<ArrowSquareOutIcon class={icon} aria-hidden="true" /></a>
	{:else}{token.value}{/if}
{/each}
