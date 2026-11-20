<script lang="ts">
  import { t } from '$lib/i18n';
	import * as Sheet from "$lib/components/ui/sheet/index.js";
	import { cn, type WithElementRef } from "$lib/utils.js";
	import type { HTMLAttributes } from "svelte/elements";
	import { SIDEBAR_WIDTH_MOBILE } from "./constants.js";
	import { useSidebar } from "./context.svelte.js";

	let {
		ref = $bindable(null),
		side = "left",
		variant = "sidebar",
		collapsible = "offcanvas",
		class: className,
		children,
		...restProps
	}: WithElementRef<HTMLAttributes<HTMLDivElement>> & {
		side?: "left" | "right";
		variant?: "sidebar" | "floating" | "inset";
		collapsible?: "offcanvas" | "icon" | "none";
	} = $props();

	const sidebar = useSidebar();
	let swipeStart: { x: number; y: number } | null = null;

	function startSwipe(event: TouchEvent): void {
		swipeStart = event.touches.length === 1
			? { x: event.touches[0].clientX, y: event.touches[0].clientY }
			: null;
	}

	function moveSwipe(event: TouchEvent): void {
		if (!swipeStart) return;
		const dx = event.touches[0].clientX - swipeStart.x;
		const dy = Math.abs(event.touches[0].clientY - swipeStart.y);
		if (event.touches.length !== 1 || (dy > 10 && dy > Math.abs(dx))) {
			swipeStart = null;
		}
	}

	function endSwipe(event: TouchEvent): void {
		if (!swipeStart) return;
		const leftwardDistance = swipeStart.x - event.changedTouches[0].clientX;
		const dy = Math.abs(event.changedTouches[0].clientY - swipeStart.y);
		swipeStart = null;
		if (leftwardDistance >= 64 && leftwardDistance > dy * 2) sidebar.setOpenMobile(false);
	}
</script>

{#if collapsible === "none"}
	<div
		class={cn(
			"bg-sidebar text-sidebar-foreground flex h-full w-(--sidebar-width) flex-col",
			className
		)}
		bind:this={ref}
		{...restProps}
	>
		{@render children?.()}
	</div>
{:else if sidebar.isMobile}
	<Sheet.Root
		bind:open={() => sidebar.openMobile, (v) => sidebar.setOpenMobile(v)}
		{...restProps}
	>
		<Sheet.Content
			bind:ref
			data-sidebar="sidebar"
			data-slot="sidebar"
			data-mobile="true"
			showCloseButton={false}
			ontouchstart={startSwipe}
			ontouchmove={moveSwipe}
			ontouchend={endSwipe}
			ontouchcancel={() => { swipeStart = null }}
			class={cn(
				"bg-sidebar text-sidebar-foreground touch-pan-y touch-pinch-zoom border-none p-0 data-[side=left]:w-(--sidebar-width) data-[side=right]:w-(--sidebar-width) data-[side=left]:sm:max-w-none data-[side=right]:sm:max-w-none",
				className
			)}
			style="--sidebar-width: {SIDEBAR_WIDTH_MOBILE};"
			{side}
		>
			<Sheet.Header class="sr-only">
				<Sheet.Title>{$t('nav.sidebar')}</Sheet.Title>
				<Sheet.Description>{$t('nav.sidebarDescription')}</Sheet.Description>
			</Sheet.Header>
			<div class="flex h-full w-full flex-col">
				{@render children?.()}
			</div>
		</Sheet.Content>
	</Sheet.Root>
{:else}
	<div
		bind:this={ref}
		class="text-sidebar-foreground group peer hidden md:block"
		data-state={sidebar.state}
		data-collapsible={sidebar.state === "collapsed" ? collapsible : ""}
		data-variant={variant}
		data-side={side}
		data-slot="sidebar"
	>
		<!-- This is what handles the sidebar gap on desktop -->
		<div
			data-slot="sidebar-gap"
			class={cn(
				"transition-[width] duration-200 ease-linear relative w-(--sidebar-width) bg-transparent",
				"group-data-[collapsible=offcanvas]:w-0",
				"group-data-[side=right]:rotate-180",
				variant === "floating" || variant === "inset"
					? "group-data-[collapsible=icon]:w-[calc(var(--sidebar-width-icon)+(--spacing(4)))]"
					: "group-data-[collapsible=icon]:w-(--sidebar-width-icon)"
			)}
		></div>
		<div
			data-slot="sidebar-container"
			class={cn(
				"fixed inset-y-0 z-10 hidden h-svh w-(--sidebar-width) transition-[left,right,width] duration-200 ease-linear md:flex",
				side === "left"
					? "start-0 group-data-[collapsible=offcanvas]:start-[calc(var(--sidebar-width)*-1)]"
					: "end-0 group-data-[collapsible=offcanvas]:end-[calc(var(--sidebar-width)*-1)]",
				// Adjust the padding for floating and inset variants.
				variant === "floating" || variant === "inset"
					? "p-2 group-data-[collapsible=icon]:w-[calc(var(--sidebar-width-icon)+(--spacing(4))+2px)]"
					: "group-data-[collapsible=icon]:w-(--sidebar-width-icon) group-data-[side=left]:border-e group-data-[side=right]:border-s",
				className
			)}
			{...restProps}
		>
			<div
				data-sidebar="sidebar"
				data-slot="sidebar-inner"
				class="bg-sidebar group-data-[variant=floating]:ring-sidebar-border group-data-[variant=floating]:rounded-lg group-data-[variant=floating]:shadow-none group-data-[variant=floating]:ring-1 flex size-full flex-col"
			>
				{@render children?.()}
			</div>
		</div>
	</div>
{/if}
