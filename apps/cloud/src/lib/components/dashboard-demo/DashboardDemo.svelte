<script lang="ts">
	import CaretRightIcon from 'phosphor-svelte/lib/CaretRightIcon';
	import CaretUpDownIcon from 'phosphor-svelte/lib/CaretUpDownIcon';
	import ArrowUpRightIcon from 'phosphor-svelte/lib/ArrowUpRightIcon';
	import DesktopIcon from 'phosphor-svelte/lib/DesktopIcon';
	import FunnelIcon from 'phosphor-svelte/lib/FunnelIcon';
	import GearIcon from 'phosphor-svelte/lib/GearIcon';
	import NotePencilIcon from 'phosphor-svelte/lib/NotePencilIcon';
	import PushPinIcon from 'phosphor-svelte/lib/PushPinIcon';
	import QuestionIcon from 'phosphor-svelte/lib/QuestionIcon';
	import SidebarSimpleIcon from 'phosphor-svelte/lib/SidebarSimpleIcon';
	import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js';
	import { cn } from '$lib/utils';
	import DemoConversation from './DemoConversation.svelte';
	import { demoDevices, type DemoSession, type DemoSessionState } from './mockData';

	let deviceHandle = $state(demoDevices[0].handle);
	// Open on a session whose conversation fits without scrolling.
	let sessionId = $state('review-api');
	let workspaceFilter = $state('');
	let mobileSidebarOpen = $state(false);

	const device = $derived(demoDevices.find((item) => item.handle === deviceHandle)!);
	const session = $derived(device.sessions.find((item) => item.id === sessionId)!);
	const pinnedSessions = $derived(device.sessions.filter((item) => item.pinned));
	// Like the Dashboard, the workspace filter only scopes Recent Sessions.
	const recentSessions = $derived(device.sessions.filter((item) => !item.pinned && (!workspaceFilter || item.workspaceId === workspaceFilter)));
	const filterWorkspace = $derived(device.workspaces.find((item) => item.id === workspaceFilter));

	const stateDot: Record<Exclude<DemoSessionState, 'exited'>, string> = { busy: 'bg-busy', idle: 'bg-idle', waiting_for_interaction: 'bg-interrupted' };
	const stateLabel: Record<Exclude<DemoSessionState, 'exited'>, string> = { busy: 'busy', idle: 'idle', waiting_for_interaction: 'waiting for input' };
	const menuButton = 'flex h-8 w-full min-w-0 items-center gap-2 overflow-hidden rounded-md p-2 text-left text-sm outline-hidden [&_svg]:size-4 [&_svg]:shrink-0';
	const dropdownContent = 'rounded-lg p-1 shadow-none';
	const dropdownItem = 'gap-1.5 rounded-sm py-1 pl-1.5 text-sm';

	function selectDevice(handle: string): void {
		if (handle === deviceHandle) return;
		deviceHandle = handle;
		workspaceFilter = '';
		sessionId = demoDevices.find((item) => item.handle === handle)!.sessions[0].id;
	}

	function selectSession(id: string): void {
		sessionId = id;
		mobileSidebarOpen = false;
	}
</script>

{#snippet sessionItem(item: DemoSession)}
	{@const active = item.id === session.id}
	<li class="relative text-sm">
		<button
			type="button"
			class={cn(menuButton, 'pr-8 hover:bg-accent hover:text-accent-foreground focus-visible:ring-2 focus-visible:ring-primary', active && 'bg-accent text-accent-foreground')}
			aria-current={active ? 'true' : undefined}
			onclick={() => selectSession(item.id)}
		>
			{#if item.pinned}<PushPinIcon class="size-3.5!" aria-label="Pinned session" />{/if}
			<span class={cn('truncate', active && 'font-medium')}>{item.title}</span>
		</button>
		{#if item.state !== 'exited'}
			<span class="pointer-events-none absolute top-1.5 right-1 flex size-5 items-center justify-center" aria-label={`${stateLabel[item.state]} session`}>
				<span class={cn('size-2 rounded-full', stateDot[item.state])}></span>
			</span>
		{/if}
	</li>
{/snippet}

{#snippet groupHeading(label: string)}
	<span class="flex items-center gap-1 text-xs font-medium">{label}<CaretRightIcon class="size-3 rotate-90" aria-hidden="true" /></span>
{/snippet}

<div class="dashboard-demo @container relative flex h-full overflow-hidden bg-surface text-foreground" role="region" aria-label="Interactive Dashboard demo with sample data">
	{#if mobileSidebarOpen}
		<button type="button" class="absolute inset-0 z-20 bg-black/40 @3xl:hidden" aria-label="Close sidebar" onclick={() => (mobileSidebarOpen = false)}></button>
	{/if}

	<aside
		class={cn(
			'absolute inset-y-0 left-0 z-30 w-72 max-w-[85%] flex-col border-e border-border bg-background @3xl:static @3xl:flex @3xl:w-60 @3xl:max-w-none',
			mobileSidebarOpen ? 'flex' : 'hidden'
		)}
		aria-label="Sidebar"
	>
		<div class="flex h-14 shrink-0 items-center gap-1 px-2" aria-hidden="true">
			<div class="flex min-w-0 flex-1 items-center gap-3 px-2 text-sm font-semibold text-heading">
				<img src="/logo.svg" alt="" class="size-8 shrink-0 object-contain" />
				<span class="truncate">Pontia</span>
			</div>
			<span class="flex size-7 items-center justify-center"><SidebarSimpleIcon class="size-4" /></span>
		</div>
		<div class="mx-4 h-px shrink-0 bg-border"></div>

		<div class="flex min-h-0 flex-1 flex-col">
			<div class="p-2" aria-hidden="true">
				<div class={cn(menuButton, 'mb-2 h-10 bg-primary text-[13px] font-semibold text-primary-foreground')}>
					<NotePencilIcon />
					<span>New Chat</span>
					<span class="ml-auto inline-flex h-5 min-w-5 items-center justify-center rounded-sm border border-primary-foreground/40 px-1 text-[10px] font-medium text-primary-foreground/80">Alt+N</span>
				</div>
			</div>

			<div class="shrink-0 p-2 pb-0 text-sm">
				<DropdownMenu.Root>
					<DropdownMenu.Trigger class={cn(menuButton, 'hover:bg-accent hover:text-accent-foreground focus-visible:ring-2 focus-visible:ring-primary data-[state=open]:bg-accent')} aria-label="Sidebar device">
						<DesktopIcon class="text-foreground/60" />
						<span class="min-w-0 flex-1 truncate font-medium">{device.name}</span>
						<CaretUpDownIcon class="text-foreground/60" />
					</DropdownMenu.Trigger>
					<DropdownMenu.Content align="start" class={cn(dropdownContent, 'min-w-56')}>
						<DropdownMenu.Label class="px-1.5 py-1 pl-7 font-medium">Devices</DropdownMenu.Label>
						<DropdownMenu.RadioGroup value={deviceHandle} onValueChange={selectDevice}>
							{#each demoDevices as item (item.handle)}
								<DropdownMenu.RadioItem closeOnSelect value={item.handle} class={cn(dropdownItem, 'pr-2 pl-7 *:data-[slot=dropdown-menu-radio-item-indicator]:right-auto *:data-[slot=dropdown-menu-radio-item-indicator]:left-2')}>
									<span class="min-w-0 flex-1 truncate">{item.name}</span>
									<span class="shrink-0 text-xs text-muted-foreground">Online</span>
								</DropdownMenu.RadioItem>
							{/each}
						</DropdownMenu.RadioGroup>
					</DropdownMenu.Content>
				</DropdownMenu.Root>
			</div>

			<div class="demo-scroll min-h-0 flex-1 overflow-y-auto">
				{#if pinnedSessions.length}
					<div class="p-2">
						<div class="flex h-8 items-center px-2 text-foreground/70">{@render groupHeading('Pinned Sessions')}</div>
						<ul class="flex flex-col gap-1 pr-1">
							{#each pinnedSessions as item (item.id)}{@render sessionItem(item)}{/each}
						</ul>
					</div>
				{/if}
				<div class="p-2">
					<div class="flex h-8 items-center gap-1 px-2 text-foreground/70">
						{@render groupHeading('Recent Sessions')}
						<span class="flex-1"></span>
						<span class="flex items-center gap-2">
							<ArrowUpRightIcon class="size-4" aria-hidden="true" />
							<DropdownMenu.Root>
								<DropdownMenu.Trigger
									class={cn('rounded-sm hover:text-accent-foreground focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-hidden', workspaceFilter && 'text-primary')}
									aria-label="Filter recent sessions by workspace"
									title={filterWorkspace ? `Workspace: ${filterWorkspace.name}` : 'All workspaces'}
								>
									<FunnelIcon class="size-4" weight={workspaceFilter ? 'fill' : 'regular'} />
								</DropdownMenu.Trigger>
								<DropdownMenu.Content side="right" align="start" class={cn(dropdownContent, 'w-72')}>
									<DropdownMenu.Label class="px-1.5 py-1 font-medium">Workspace</DropdownMenu.Label>
									<DropdownMenu.RadioGroup bind:value={workspaceFilter}>
										<DropdownMenu.RadioItem closeOnSelect value="" class={dropdownItem}>All workspaces</DropdownMenu.RadioItem>
										<DropdownMenu.Separator class="my-1" />
										{#each device.workspaces as workspace (workspace.id)}
											<DropdownMenu.RadioItem closeOnSelect value={workspace.id} class={dropdownItem}>
												<span class="min-w-0">
													<span class="block truncate">{workspace.name}</span>
													<span class="block truncate text-xs text-muted-foreground">{workspace.path}</span>
												</span>
											</DropdownMenu.RadioItem>
										{/each}
									</DropdownMenu.RadioGroup>
								</DropdownMenu.Content>
							</DropdownMenu.Root>
						</span>
					</div>
					<ul class="flex flex-col gap-1 pr-1">
						{#each recentSessions as item (item.id)}{@render sessionItem(item)}{:else}
							<li class="px-2 py-1 text-xs text-foreground/60">No recent sessions</li>
						{/each}
					</ul>
				</div>
			</div>
		</div>

		<div class="relative p-2" aria-hidden="true">
			<div class={cn(menuButton, 'h-9 px-3 text-[13px]')}><GearIcon /><span>Settings</span></div>
			<span class="absolute top-3.5 right-3 flex size-5 items-center justify-center"><QuestionIcon class="size-4" /></span>
		</div>
	</aside>

	<div class="flex min-w-0 flex-1 flex-col">
		<div class="flex h-12 shrink-0 items-center px-3 @3xl:hidden">
			<div class="flex h-8 min-w-0 items-center gap-1 rounded-lg bg-background px-1 ring-1 ring-foreground/10">
				<button type="button" class="flex size-7 shrink-0 items-center justify-center rounded-md hover:bg-muted" aria-label="Open sidebar" aria-expanded={mobileSidebarOpen} onclick={() => (mobileSidebarOpen = true)}>
					<SidebarSimpleIcon class="size-4" />
				</button>
			</div>
		</div>
		{#key session.id}
			<DemoConversation {session} workspace={device.workspaces.find((item) => item.id === session.workspaceId)} />
		{/key}
	</div>
</div>

<style>
	/* The site styles focused buttons with an offset outline; the demo uses the Dashboard's focus ring. */
	.dashboard-demo :global(button:focus-visible) {
		outline: none;
	}
	/* Chromium ignores ::-webkit-scrollbar when scrollbar-width or scrollbar-color is set, and its thin width is wider than 6px. */
	@supports not selector(::-webkit-scrollbar) {
		.dashboard-demo :global(.demo-scroll) {
			scrollbar-width: thin;
			scrollbar-color: color-mix(in srgb, var(--border) 75%, transparent) transparent;
		}
	}
	.dashboard-demo :global(.demo-scroll::-webkit-scrollbar) {
		width: 6px;
		height: 6px;
	}
	.dashboard-demo :global(.demo-scroll::-webkit-scrollbar-track) {
		background: transparent;
	}
	.dashboard-demo :global(.demo-scroll::-webkit-scrollbar-thumb) {
		border-radius: 3px;
		background-color: color-mix(in srgb, var(--border) 75%, transparent);
	}
	.dashboard-demo :global(.demo-scroll::-webkit-scrollbar-thumb:hover) {
		background-color: color-mix(in srgb, var(--muted-foreground) 55%, transparent);
	}
</style>
