<script lang="ts">
	import * as Dialog from '$lib/components/ui/dialog';
	import XIcon from 'phosphor-svelte/lib/XIcon';

	/** The mobile Dashboard's chat action sheet, reduced to the rows the demo can open. */
	let { title, subtitle, rows, onclose }: {
		title: string;
		subtitle: string;
		rows: { label: string; detail?: string; onselect: () => void }[];
		onclose: () => void;
	} = $props();
</script>

<Dialog.Root open onOpenChange={(open) => { if (!open) onclose(); }}>
	<Dialog.Content portalProps={{ disabled: true }} showCloseButton={false} preventScroll={false}
		onCloseAutoFocus={(event) => event.preventDefault()}
		class="demo-sheet-enter absolute inset-x-0 top-auto bottom-0 left-0 block w-full max-w-none translate-x-0 translate-y-0 rounded-t-[22px] rounded-b-none bg-background px-4 pt-2 pb-3 text-foreground shadow-[0_-8px_30px_rgb(0_0_0/12%)] sm:max-w-none">
		<span class="mx-auto mb-2 block h-1 w-9 rounded-full bg-border" aria-hidden="true"></span>
		<div class="flex items-start gap-2 pb-1">
			<div class="min-w-0 flex-1">
				<Dialog.Title class="truncate text-base font-semibold text-heading">{title}</Dialog.Title>
				<Dialog.Description class="truncate text-xs text-muted-foreground">{subtitle}</Dialog.Description>
			</div>
			<button type="button" class="grid size-8 place-items-center rounded-full hover:bg-muted" aria-label="Close" onclick={onclose}><XIcon class="size-4" /></button>
		</div>
		{#each rows as row}
			<button type="button" class="flex min-h-11 w-full items-center gap-3 border-b border-border text-left text-sm last:border-0" onclick={row.onselect}>
				{row.label}{#if row.detail}<span class="ml-auto text-muted-foreground">{row.detail}</span>{/if}
			</button>
		{/each}
	</Dialog.Content>
</Dialog.Root>

<style>
	@media (prefers-reduced-motion: no-preference) {
		:global(.demo-sheet-enter) { animation: demo-sheet-enter 200ms ease-out; }
	}
	@keyframes demo-sheet-enter {
		from { transform: translateY(100%); }
	}
</style>
