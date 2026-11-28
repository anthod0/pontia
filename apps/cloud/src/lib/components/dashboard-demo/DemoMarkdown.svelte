<script lang="ts">
	import CopyIcon from 'phosphor-svelte/lib/CopyIcon';
	import DemoInline from './DemoInline.svelte';
	import { markdownBlocks } from './text';

	/** A Markdown file as the Dashboard's file preview renders it. */
	let { source }: { source: string } = $props();
	const blocks = $derived(markdownBlocks(source));
</script>

<div class="min-w-0 text-sm leading-[1.85] text-foreground [&>*:first-child]:mt-0 [&>*:last-child]:mb-0">
	{#each blocks as block}
		{#if block.kind === 'h1'}<h1 class="mt-6 mb-4 text-2xl font-bold text-heading"><DemoInline text={block.text} /></h1>
		{:else if block.kind === 'h2'}<h2 class="mt-5 mb-3 text-xl font-semibold text-heading"><DemoInline text={block.text} /></h2>
		{:else if block.kind === 'h3'}<h3 class="mt-4 mb-2 text-lg font-semibold text-heading"><DemoInline text={block.text} /></h3>
		{:else if block.kind === 'p'}<p class="my-3"><DemoInline text={block.text} /></p>
		{:else if block.kind === 'quote'}<blockquote class="my-4 border-l-2 border-border pl-3 text-muted-foreground"><DemoInline text={block.text} /></blockquote>
		{:else if block.kind === 'ul'}
			<ul class="my-3 list-disc pl-5">{#each block.items as item}<li class="my-1"><DemoInline text={item} /></li>{/each}</ul>
		{:else if block.kind === 'code'}
			<div class="my-4 overflow-hidden rounded-lg border border-border bg-muted">
				<div class="flex items-center justify-between gap-3 px-3 py-1.5 font-mono text-[11px] text-muted-foreground">
					<span class="font-medium tracking-wide uppercase">{block.language}</span>
					<span class="inline-flex items-center gap-1.5 px-2 py-1 font-sans text-xs font-medium" aria-hidden="true"><CopyIcon class="size-3.5" />Copy</span>
				</div>
				<pre class="demo-scroll overflow-x-auto px-4 pt-1 pb-4 font-mono text-xs leading-relaxed text-foreground"><code>{block.code}</code></pre>
			</div>
		{:else if block.kind === 'table'}
			<div class="demo-scroll my-4 max-w-full overflow-x-auto">
				<table class="w-full border-collapse text-left">
					<thead><tr>{#each block.head as cell}<th class="border border-border bg-muted px-3 py-2 font-semibold text-heading"><DemoInline text={cell} /></th>{/each}</tr></thead>
					<tbody>{#each block.rows as row}<tr>{#each row as cell}<td class="border border-border px-3 py-2"><DemoInline text={cell} /></td>{/each}</tr>{/each}</tbody>
				</table>
			</div>
		{/if}
	{/each}
</div>
