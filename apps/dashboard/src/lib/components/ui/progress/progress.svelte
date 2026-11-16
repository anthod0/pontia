<script lang="ts">
	import { Progress as ProgressPrimitive } from "bits-ui";
	import { cn, type WithoutChildrenOrChild } from "$lib/utils.js";

	let {
		ref = $bindable(null),
		class: className,
		max = 100,
		value,
		...restProps
	}: WithoutChildrenOrChild<ProgressPrimitive.RootProps> = $props();
</script>

<ProgressPrimitive.Root
	bind:ref
	data-slot="progress"
	class={cn("bg-muted h-1 rounded-full relative flex w-full items-center overflow-x-hidden", className)}
	{value}
	{max}
	{...restProps}
>
	{#if value === null}
		<div data-slot="progress-indicator" class="bg-primary h-full w-1/3 animate-pulse"></div>
	{:else}
		<div
			data-slot="progress-indicator"
			class="bg-primary h-full w-full flex-1 transition-transform"
			style:transform={`translateX(-${100 - (100 * (value ?? 0)) / (max ?? 1)}%)`}
		></div>
	{/if}
</ProgressPrimitive.Root>
