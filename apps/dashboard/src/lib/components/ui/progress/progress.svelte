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
	<div
		data-slot="progress-indicator"
		class={cn("bg-primary h-full transition-all", value === null ? "w-1/3 animate-pulse" : "w-full flex-1")}
		style:transform={value === null ? undefined : `translateX(-${100 - (100 * (value ?? 0)) / (max ?? 1)}%)`}
	></div>
</ProgressPrimitive.Root>
