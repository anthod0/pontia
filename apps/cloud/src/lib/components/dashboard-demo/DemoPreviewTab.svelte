<script lang="ts">
	import ArrowClockwiseIcon from 'phosphor-svelte/lib/ArrowClockwiseIcon';
	import ArrowSquareOutIcon from 'phosphor-svelte/lib/ArrowSquareOutIcon';
	import DesktopIcon from 'phosphor-svelte/lib/DesktopIcon';
	import DeviceMobileIcon from 'phosphor-svelte/lib/DeviceMobileIcon';
	import GlobeIcon from 'phosphor-svelte/lib/GlobeIcon';
	import * as Empty from '$lib/components/ui/empty/index.js';
	import * as Tabs from '$lib/components/ui/tabs/index.js';
	import { cn } from '$lib/utils';
	import DemoLocalhostApp from './DemoLocalhostApp.svelte';

	/** The Preview tab: a localhost link from the chat, shown in a simulated page that scales like the Dashboard's iframe. */
	let { url = $bindable() }: { url: string | null } = $props();
	let address = $derived(url ?? '');
	let rejected = $state<string | null>(null);
	const invalid = $derived(address === rejected);
	let mobile = $state(false);
	let width = $state(0);
	let reloads = $state(0);
	// Like the Dashboard: a 1024px desktop viewport or a 375px phone viewport, scaled down to the panel.
	const viewport = $derived(mobile ? 375 : Math.max(1024, width));
	const scale = $derived(width ? Math.min(1, width / viewport) : 1);
	const iconButton = 'flex size-7 shrink-0 items-center justify-center rounded-full hover:bg-muted focus-visible:ring-2 focus-visible:ring-primary aria-pressed:bg-muted';

	function navigate(current: string) {
		const next = URL.parse(address, current);
		if (!next || next.origin !== new URL(current).origin) {
			rejected = address;
			return;
		}
		rejected = null;
		address = next.href;
		url = next.href;
	}
</script>

<Tabs.Content value="preview" class="flex min-h-0 flex-col">
	{#if url}
		{@const current = url}
		<div class="flex h-12 shrink-0 items-center gap-1 border-b border-border px-2">
			<form class="min-w-0 flex-1" onsubmit={(event) => { event.preventDefault(); navigate(current); }}>
				<input
					aria-label="Preview address"
					aria-invalid={invalid}
					bind:value={address}
					oninput={() => (rejected = null)}
					class="h-8 w-full min-w-0 rounded-(--radius-field) border border-border bg-transparent px-2.5 text-sm outline-none focus-visible:border-primary focus-visible:ring-3 focus-visible:ring-primary/20 aria-invalid:border-destructive"
				/>
			</form>
			<button type="button" class={iconButton} aria-label="Refresh" title="Refresh" onclick={() => reloads++}><ArrowClockwiseIcon class="size-4" /></button>
			<button type="button" class={iconButton} aria-label="Mobile viewport" title="Mobile viewport" aria-pressed={mobile} onclick={() => (mobile = !mobile)}>
				{#if mobile}<DeviceMobileIcon class="size-4" />{:else}<DesktopIcon class="size-4" />{/if}
			</button>
			<span class={cn(iconButton, 'text-muted-foreground')} title="Open in new tab" aria-hidden="true"><ArrowSquareOutIcon class="size-4" /></span>
		</div>
		<div bind:clientWidth={width} class="relative flex min-h-0 flex-1 justify-center overflow-hidden bg-muted">
			{#key reloads}
				<div class="demo-preview-page max-w-none shrink-0 origin-top overflow-auto bg-white" style:width={viewport + 'px'} style:height={100 / scale + '%'} style:transform={`scale(${scale})`}>
					<DemoLocalhostApp path={new URL(current).pathname} {mobile} onnavigate={(path) => (url = new URL(path, current).href)} />
				</div>
			{/key}
		</div>
	{:else}
		<Empty.Root class="min-h-0 flex-none p-8">
			<Empty.Header>
				<Empty.Media class="size-12 rounded-xl bg-muted text-muted-foreground"><GlobeIcon class="size-6" /></Empty.Media>
				<Empty.Title class="text-sm">Open a localhost link in the chat to preview it here.</Empty.Title>
			</Empty.Header>
		</Empty.Root>
	{/if}
</Tabs.Content>
