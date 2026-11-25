<script lang="ts">
	import CaretLeftIcon from 'phosphor-svelte/lib/CaretLeftIcon';

	/** The phone layout's header for a pushed screen: a round back button and a glass title pill, as in the mobile Dashboard. */
	let { title, subtitle, onback }: { title: string; subtitle?: string; onback: () => void } = $props();
</script>

<div class="flex shrink-0 items-start gap-2 px-3 pt-3 pb-2">
	<button type="button" class="demo-glass mt-1 grid size-10 shrink-0 place-items-center rounded-full text-foreground focus-visible:ring-2 focus-visible:ring-primary" aria-label="Back" onclick={onback}>
		<CaretLeftIcon class="size-[18px]" />
	</button>
	<div class="demo-glass min-w-0 rounded-[22px] px-4 py-2">
		<p class="truncate text-[15px] leading-5 font-semibold text-heading">{title}</p>
		{#if subtitle}<p class="truncate text-xs leading-4 text-muted-foreground">{subtitle}</p>{/if}
	</div>
</div>
