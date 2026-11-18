<script lang="ts">
  import { t } from '$lib/i18n';
	import { Button } from "$lib/components/ui/button/index.js";
	import CaretLeftIcon from 'phosphor-svelte/lib/CaretLeftIcon';
	import XIcon from 'phosphor-svelte/lib/XIcon';
	import SidebarSimpleIcon from 'phosphor-svelte/lib/SidebarSimpleIcon';
	import { cn } from "$lib/utils.js";
	import type { ComponentProps } from "svelte";
	import { useSidebar } from "./context.svelte.js";

	let {
		ref = $bindable(null),
		class: className,
		onclick,
		...restProps
	}: ComponentProps<typeof Button> & {
		onclick?: (e: MouseEvent) => void;
	} = $props();

	const sidebar = useSidebar();
</script>

<Button
	bind:ref
	data-sidebar="trigger"
	data-slot="sidebar-trigger"
	variant="ghost"
	size="icon-sm"
	class={cn("cn-sidebar-trigger", className)}
	type="button"
	onclick={(e) => {
		onclick?.(e);
		sidebar.toggle();
	}}
	{...restProps}
>
	{#if sidebar.isMobile}
		{#if sidebar.openMobile}<XIcon />{:else}<CaretLeftIcon />{/if}
	{:else}
		<SidebarSimpleIcon />
	{/if}
	<span class="sr-only">{$t(sidebar.isMobile && sidebar.openMobile ? 'common.close' : 'nav.toggleSidebar')}</span>
</Button>
