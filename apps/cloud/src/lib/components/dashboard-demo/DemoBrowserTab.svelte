<script lang="ts">
	import ArrowClockwiseIcon from 'phosphor-svelte/lib/ArrowClockwiseIcon';
	import XIcon from 'phosphor-svelte/lib/XIcon';
	import DemoLocalhostApp from './DemoLocalhostApp.svelte';

	/**
	 * On phones the mobile Dashboard opens a localhost link in a new browser tab.
	 * The demo shows that tab inside the phone frame, with neutral browser chrome so it reads as outside Pontia.
	 */
	let { url = $bindable(), onclose }: { url: string; onclose: () => void } = $props();
	let width = $state(0);
	let reloads = $state(0);
	const scale = $derived(width ? Math.min(1, width / 375) : 1);
	const address = $derived(new URL(url).host + new URL(url).pathname);
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<section class="demo-tab-enter absolute inset-0 z-40 flex flex-col bg-[#f2f2f0] dark:bg-[#1f1f1f]" aria-label="Browser tab: {address}" onkeydown={(event) => { if (event.key === 'Escape') onclose(); }}>
	<div class="flex shrink-0 items-center gap-2 px-2.5 pt-2.5 pb-2">
		<button type="button" class="grid size-8 shrink-0 place-items-center rounded-full text-[#3c3c3c] hover:bg-black/5 focus-visible:ring-2 focus-visible:ring-primary dark:text-[#e5e5e5] dark:hover:bg-white/10" aria-label="Close tab and return to the Dashboard" title="Back to Dashboard" {@attach (node) => node.focus({ preventScroll: true })} onclick={onclose}><XIcon class="size-4" /></button>
		<span class="flex h-8 min-w-0 flex-1 items-center justify-center rounded-full bg-white px-3 font-sans text-[12.5px] text-[#3c3c3c] shadow-[0_0_0_1px_rgb(0_0_0/6%)] dark:bg-[#333] dark:text-[#e5e5e5]">
			<span class="truncate">{address}</span>
		</span>
		<button type="button" class="grid size-8 shrink-0 place-items-center rounded-full text-[#3c3c3c] hover:bg-black/5 dark:text-[#e5e5e5] dark:hover:bg-white/10" aria-label="Reload" onclick={() => reloads++}><ArrowClockwiseIcon class="size-4" /></button>
	</div>
	<div bind:clientWidth={width} class="relative min-h-0 flex-1 overflow-hidden">
		{#key reloads}
			<div class="demo-preview-page origin-top-left overflow-auto bg-white" style:width="375px" style:height={100 / scale + '%'} style:transform={`scale(${scale})`}>
				<DemoLocalhostApp path={new URL(url).pathname} mobile onnavigate={(path) => (url = new URL(path, url).href)} />
			</div>
		{/key}
	</div>
</section>

<style>
	@media (prefers-reduced-motion: no-preference) {
		.demo-tab-enter { animation: demo-tab-enter 220ms ease-out; }
	}
	@keyframes demo-tab-enter {
		from { transform: translateY(6%); opacity: 0; }
	}
</style>
