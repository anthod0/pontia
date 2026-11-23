<script lang="ts">
	import ArrowClockwiseIcon from 'phosphor-svelte/lib/ArrowClockwiseIcon';
	import FileTextIcon from 'phosphor-svelte/lib/FileTextIcon';
	import XIcon from 'phosphor-svelte/lib/XIcon';
	import * as Empty from '$lib/components/ui/empty/index.js';
	import * as Tabs from '$lib/components/ui/tabs/index.js';

	let { files, file, recent, description, onopen, onclose, onclear }: {
		files: Record<string, string>;
		file: string | null;
		recent: string[];
		description?: string;
		onopen: (path: string) => void;
		onclose: () => void;
		onclear: () => void;
	} = $props();

	function split(path: string): { name: string; dir: string } {
		const slash = path.lastIndexOf('/');
		return { name: path.slice(slash + 1), dir: slash < 0 ? '' : path.slice(0, slash) };
	}
</script>

<Tabs.Content value="files" class="flex min-h-0 flex-col text-sm">
	{#if file}
		{@const parts = split(file)}
		<header class="flex shrink-0 items-center gap-1 border-b border-border px-2 py-2">
			<h2 class="panel-heading min-w-0 flex-1 truncate px-1 font-medium" title={file}>{parts.name}</h2>
			<span class="flex size-7 items-center justify-center" aria-hidden="true"><ArrowClockwiseIcon class="size-4" /></span>
			<button type="button" class="flex size-7 shrink-0 items-center justify-center rounded-md text-foreground hover:bg-muted focus-visible:ring-2 focus-visible:ring-primary" aria-label="Close file" title="Close file" onclick={onclose}><XIcon class="size-4" /></button>
		</header>
		<pre class="demo-scroll min-h-0 flex-1 overflow-auto p-4 font-mono text-xs leading-relaxed whitespace-pre-wrap text-foreground">{files[file]}</pre>
	{:else if recent.length}
		<section class="demo-scroll min-h-0 flex-1 overflow-auto p-3" aria-label="Recently opened">
			<div class="flex items-center justify-between gap-2 pb-1 pl-2">
				<h2 class="panel-heading text-xs! font-medium text-muted-foreground">Recently opened</h2>
				<button type="button" class="inline-flex h-6 items-center rounded-md px-2 text-xs font-medium hover:bg-muted focus-visible:ring-2 focus-visible:ring-primary" onclick={onclear}>Clear list</button>
			</div>
			<ul>
				{#each recent as path (path)}
					{@const parts = split(path)}
					<li>
						<button type="button" class="flex w-full items-center gap-3 rounded-lg p-2 text-left hover:bg-muted focus-visible:ring-2 focus-visible:ring-primary" title={path} aria-label={`Preview ${parts.name}`} onclick={() => onopen(path)}>
							<span class="grid size-9 shrink-0 place-items-center rounded-lg bg-primary/12 text-primary"><FileTextIcon class="size-5" weight="duotone" /></span>
							<span class="min-w-0 flex-1">
								<span class="block truncate text-sm font-medium">{parts.name}</span>
								{#if parts.dir}<span class="block truncate text-xs text-muted-foreground" dir="rtl"><bdi>{parts.dir}</bdi></span>{/if}
							</span>
						</button>
					</li>
				{/each}
			</ul>
		</section>
	{:else}
		<Empty.Root class="min-h-0 flex-none p-8">
			<Empty.Header>
				<Empty.Media class="size-12 rounded-xl bg-muted text-muted-foreground"><FileTextIcon class="size-6" /></Empty.Media>
				<Empty.Title class="text-sm">No open file</Empty.Title>
				{#if description}<Empty.Description class="text-sm">{description}</Empty.Description>{/if}
			</Empty.Header>
		</Empty.Root>
	{/if}
</Tabs.Content>

<style>
	/* The site's section headings style every h2; the panel keeps the Dashboard's size. */
	.panel-heading {
		font-size: 14px;
		line-height: 20px;
		letter-spacing: normal;
	}
</style>
