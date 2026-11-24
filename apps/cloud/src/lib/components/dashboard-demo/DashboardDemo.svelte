<script lang="ts">
	import ArrowUpRightIcon from 'phosphor-svelte/lib/ArrowUpRightIcon';
	import * as Tabs from '$lib/components/ui/tabs';
	import ArrowsLeftRightIcon from 'phosphor-svelte/lib/ArrowsLeftRightIcon';
	import CaretLeftIcon from 'phosphor-svelte/lib/CaretLeftIcon';
	import CaretRightIcon from 'phosphor-svelte/lib/CaretRightIcon';
	import CaretUpDownIcon from 'phosphor-svelte/lib/CaretUpDownIcon';
	import DesktopIcon from 'phosphor-svelte/lib/DesktopIcon';
	import DotsThreeIcon from 'phosphor-svelte/lib/DotsThreeIcon';
	import FunnelIcon from 'phosphor-svelte/lib/FunnelIcon';
	import GearIcon from 'phosphor-svelte/lib/GearIcon';
	import NotePencilIcon from 'phosphor-svelte/lib/NotePencilIcon';
	import PushPinIcon from 'phosphor-svelte/lib/PushPinIcon';
	import QuestionIcon from 'phosphor-svelte/lib/QuestionIcon';
	import SidebarSimpleIcon from 'phosphor-svelte/lib/SidebarSimpleIcon';
	import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js';
	import { cn } from '$lib/utils';
	import BotAvatarMark from '$lib/components/bot-story/BotAvatarMark.svelte';
	import DemoSetup, { settingsSections, type DemoSettingsSection } from './DemoSetup.svelte';
	import DemoBotChat from './DemoBotChat.svelte';
	import DemoConversation from './DemoConversation.svelte';
	import DemoSessionPanel from './DemoSessionPanel.svelte';
	import { demoDevices, type DemoChange, type DemoSession, type DemoSessionState } from './mockData';

	const devices = $state(demoDevices);
	let deviceHandle = $state(demoDevices[0].handle);
	let previousDeviceHandle = $state<string | null>(null);
	let panelOpen = $state(false);
	let panelTab = $state<'files' | 'git'>('files');
	let file = $state<string | null>(null);
	let diff = $state<DemoChange | null>(null);
	let panelButton = $state<HTMLButtonElement>();
	// Open on a session whose conversation fits without scrolling.
	let sessionId = $state('review-api');
	let workspaceFilter = $state('');
	// Like the Dashboard, the Session list shows only active sessions until the filter asks for all.
	let sessionScope = $state<'active' | 'all'>('active');
	let settingsSection = $state<DemoSettingsSection>('Common');
	let setup = $state<'new' | 'settings' | 'sessions' | null>(null);
	let sessionsOpen = $state(true);
	let mobileSidebarOpen = $state(false);
	let botId = $state<string | null>(null);

	const device = $derived(devices.find((item) => item.handle === deviceHandle)!);
	const session = $derived(device.sessions.find((item) => item.id === sessionId)!);
	const pinnedSessions = $derived(device.sessions.filter((item) => item.pinned));
	// Like the Dashboard, pinned Sessions remain visible across workspace filters.
	const scopedSessions = $derived(device.sessions.filter((item) => !item.pinned && (!workspaceFilter || item.workspaceId === workspaceFilter)));
	const recentSessions = $derived(sessionScope === 'all' ? scopedSessions : scopedSessions.filter((item) => item.state !== 'exited'));
	const filterApplied = $derived(Boolean(workspaceFilter) || sessionScope === 'all');
	const filterWorkspace = $derived(device.workspaces.find((item) => item.id === workspaceFilter));
	const workspace = $derived(device.workspaces.find((item) => item.id === session.workspaceId)!);
	const previousDevice = $derived(devices.find((item) => item.handle === previousDeviceHandle));
	const bot = $derived(device.bots.find((item) => item.id === botId));

	const stateDot: Record<Exclude<DemoSessionState, 'exited'>, string> = { busy: 'bg-busy', idle: 'bg-idle', waiting_for_interaction: 'bg-interrupted' };
	const stateLabel: Record<Exclude<DemoSessionState, 'exited'>, string> = { busy: 'busy', idle: 'idle', waiting_for_interaction: 'waiting for input' };
	const menuButton = 'flex h-8 max-md:min-h-10 w-full min-w-0 items-center gap-2 overflow-hidden rounded-md p-2 text-left text-sm outline-hidden [&_svg]:size-4 [&_svg]:shrink-0';
	const dropdownContent = 'rounded-lg p-1 shadow-none';
	const dropdownItem = 'gap-1.5 rounded-sm py-1 pl-1.5 text-sm';

	function openView(view: 'new' | 'settings' | 'sessions') {
		setup = view;
		mobileSidebarOpen = false;
	}

	function selectDevice(handle: string): void {
		if (handle === deviceHandle) return;
		setup = null;
		previousDeviceHandle = deviceHandle;
		deviceHandle = handle;
		workspaceFilter = '';
		sessionId = devices.find((item) => item.handle === handle)!.sessions[0].id;
		botId = null;
		panelOpen = false;
		file = null;
		diff = null;
	}

	function selectSession(next: DemoSession): void {
		setup = null;
		if (next.workspaceId !== session.workspaceId) diff = null;
		if (next.id !== sessionId) file = null;
		sessionId = next.id;
		botId = null;
		mobileSidebarOpen = false;
	}

	function selectBot(id: string): void {
		setup = null;
		botId = id;
		mobileSidebarOpen = false;
	}

	function openFile(path: string) {
		file = path;
		session.recentFiles = [path, ...(session.recentFiles ?? []).filter((item) => item !== path)];
		panelTab = 'files';
		panelOpen = true;
	}

	function closePanel() {
		panelOpen = false;
		panelButton?.focus({ preventScroll: true });
	}

	function shortcut(event: KeyboardEvent) {
		if (event.key === 'Escape' && mobileSidebarOpen) {
			mobileSidebarOpen = false;
			event.preventDefault();
			return;
		}
		if (event.altKey && !event.ctrlKey && !event.metaKey && !event.shiftKey) {
			if (event.code === 'KeyN') {
				event.preventDefault();
				openView('new');
				return;
			}
			if (event.code === 'KeyM' && previousDevice) {
				event.preventDefault();
				selectDevice(previousDevice.handle);
				return;
			}
		}
		if (setup || bot || !event.altKey || event.ctrlKey || event.shiftKey || event.metaKey || event.code !== 'KeyG') return;
		event.preventDefault();
		if (panelOpen && panelTab === 'git') closePanel();
		else {
			panelTab = 'git';
			panelOpen = true;
		}
	}
</script>

{#snippet sessionItem(item: DemoSession)}
	{@const active = !setup && !bot && item.id === session.id}
	<li class="relative text-sm">
		<button
			type="button"
			class={cn(menuButton, 'pr-8 hover:bg-accent hover:text-accent-foreground focus-visible:ring-2 focus-visible:ring-primary', active && 'bg-accent text-accent-foreground')}
			aria-current={active ? 'true' : undefined}
			onclick={() => selectSession(item)}
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

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div onkeydown={shortcut} class="dashboard-demo demo-git-colors @container relative flex h-full overflow-hidden bg-surface text-foreground" role="region" aria-label="Interactive Dashboard demo with sample data">
	{#if mobileSidebarOpen}
		<button type="button" class="absolute inset-0 z-20 bg-black/40 @3xl:hidden" aria-label="Close sidebar" onclick={() => (mobileSidebarOpen = false)}></button>
	{/if}

	<aside
		class={cn(
			'absolute inset-y-0 left-0 z-30 flex w-72 max-w-[85%] flex-col transition-transform motion-reduce:transition-none border-e border-border bg-background @3xl:static @3xl:flex @3xl:w-60 @3xl:max-w-none',
			mobileSidebarOpen ? 'visible translate-x-0' : 'invisible -translate-x-full @3xl:visible @3xl:translate-x-0'
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
			<div class="relative shrink-0 p-2 pb-0 text-sm">
				<DropdownMenu.Root>
					<DropdownMenu.Trigger class={cn(menuButton, 'hover:bg-accent hover:text-accent-foreground focus-visible:ring-2 focus-visible:ring-primary data-[state=open]:bg-accent', previousDevice && 'pr-8')} aria-label="Sidebar device">
						<span class="relative"><DesktopIcon class="text-foreground/60" /><span class="absolute -right-0.5 bottom-0 size-1.5 rounded-full bg-idle ring-1 ring-background" aria-label="Online"></span></span>
						<span class="min-w-0 flex-1 truncate font-medium">{device.name}</span>
						<CaretUpDownIcon class="text-foreground/60" />
					</DropdownMenu.Trigger>
					<DropdownMenu.Content align="start" class={cn(dropdownContent, 'min-w-56')}>
						<DropdownMenu.Label class="px-1.5 py-1 pl-7 font-medium">Devices</DropdownMenu.Label>
						<DropdownMenu.RadioGroup value={deviceHandle} onValueChange={selectDevice}>
							{#each devices as item (item.handle)}
								<DropdownMenu.RadioItem closeOnSelect value={item.handle} class={cn(dropdownItem, 'pr-2 pl-7 *:data-[slot=dropdown-menu-radio-item-indicator]:right-auto *:data-[slot=dropdown-menu-radio-item-indicator]:left-2')}>
									<span class="min-w-0 flex-1 truncate">{item.name}</span>
									<span class="shrink-0 text-xs text-muted-foreground">Online</span>
								</DropdownMenu.RadioItem>
							{/each}
						</DropdownMenu.RadioGroup>
					</DropdownMenu.Content>
				</DropdownMenu.Root>
				{#if previousDevice}
					<button
						type="button"
						class="absolute top-3.5 right-3 flex size-5 items-center justify-center rounded-md hover:bg-accent focus-visible:ring-2 focus-visible:ring-primary [&_svg]:size-4"
						aria-label={`Switch back to ${previousDevice.name}`}
						title={`Switch back to ${previousDevice.name} (Online, Alt+M)`}
						onclick={() => selectDevice(previousDevice.handle)}
					>
						<ArrowsLeftRightIcon />
					</button>
				{/if}
			</div>

			<div class="demo-scroll min-h-0 flex-1 overflow-y-auto">
				{#if setup === 'settings'}
					<nav aria-label="Settings sections" class="space-y-1 p-2">
						{#each settingsSections as name}
							<button type="button" class={cn(menuButton, 'hover:bg-muted', settingsSection === name && 'bg-accent font-medium')} aria-current={settingsSection === name ? 'page' : undefined} onclick={() => settingsSection = name}>{name}</button>
						{/each}
					</nav>
				{:else}
				<div class="p-2">
					<div class="flex h-8 items-center px-2 text-xs font-semibold text-heading">Bots</div>
					<ul class="flex flex-col gap-1">
						{#each device.bots as item (item.id)}
							{@const active = !setup && item.id === bot?.id}
							{@const todos = item.entries.filter(entry => entry.kind === 'bot' && entry.todo === 'open').length}
							<li>
								<button
									type="button"
									class={cn(menuButton, 'group/bot h-auto min-h-12 max-md:min-h-14 gap-2.5 [&_svg]:size-7 hover:bg-accent hover:text-accent-foreground focus-visible:ring-2 focus-visible:ring-primary', active && 'bg-accent font-medium text-accent-foreground')}
									aria-current={active ? 'page' : undefined}
									onclick={() => selectBot(item.id)}
								>
									<span class="relative size-7 shrink-0">
										<BotAvatarMark shape={item.shape} color={item.color} />
										{#if todos}
											<span role="img" aria-label={`${todos} open todos`} class={cn('pointer-events-none absolute -top-[3px] -right-[3px] size-2 rounded-full bg-primary ring-2 group-hover/bot:ring-accent max-md:size-2.5', active ? 'ring-accent' : 'ring-background')}></span>
										{/if}
									</span>
									<span class="flex min-w-0 flex-1 flex-col">
										<span class="truncate font-medium">{item.name}</span>
										{#if item.working}<span class="truncate text-xs text-primary">{item.working}</span>{/if}
									</span>
								</button>
							</li>
						{/each}
					</ul>
				</div>
				<div class="p-2">
					<div class="flex h-8 items-center gap-1 px-2 text-foreground/70">
						<button type="button" class="flex items-center gap-1 text-xs font-medium" aria-expanded={sessionsOpen} onclick={() => sessionsOpen = !sessionsOpen}>Sessions<CaretRightIcon class={cn('size-3', sessionsOpen && 'rotate-90')} /></button>
						<span class="flex-1"></span>
						<span class="flex items-center gap-2">
							<button type="button" class="grid size-8 place-items-center rounded hover:bg-muted" aria-label="New Chat" onclick={() => openView('new')}><NotePencilIcon class="size-4" /></button>
							<button type="button" class="grid size-8 place-items-center rounded hover:bg-muted" aria-label="Open all sessions" onclick={() => openView('sessions')}><ArrowUpRightIcon class="size-4" /></button>
							<DropdownMenu.Root>
								<DropdownMenu.Trigger
									class={cn('rounded-sm hover:text-accent-foreground focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-hidden', filterApplied && 'text-primary')}
									aria-label="Filter sessions"
									title={`${sessionScope === 'active' ? 'Active sessions' : 'All sessions'} · ${filterWorkspace?.name ?? 'All workspaces'}`}
								>
									<FunnelIcon class="size-4" weight={filterApplied ? 'fill' : 'regular'} />
								</DropdownMenu.Trigger>
								<DropdownMenu.Content side="bottom" align="end" class={cn(dropdownContent, 'w-72')}>
									<DropdownMenu.Label class="px-1.5 py-1 font-medium">Status</DropdownMenu.Label>
									<DropdownMenu.RadioGroup bind:value={sessionScope}>
										<DropdownMenu.RadioItem closeOnSelect value="active" class={dropdownItem}>Active sessions</DropdownMenu.RadioItem>
										<DropdownMenu.RadioItem closeOnSelect value="all" class={dropdownItem}>All sessions</DropdownMenu.RadioItem>
									</DropdownMenu.RadioGroup>
									<DropdownMenu.Separator class="my-1" />
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
					{#if sessionsOpen}
					<ul class="flex flex-col gap-1 pr-1">
						{#each pinnedSessions as item (item.id)}{@render sessionItem(item)}{/each}
						{#each recentSessions as item (item.id)}{@render sessionItem(item)}{:else}
							<li class="px-2 py-1 text-xs text-foreground/60">{sessionScope === 'active' ? 'No active sessions' : 'No recent sessions'}</li>
						{/each}
						{#if recentSessions.length < scopedSessions.length}
							<li>
								<button
									type="button"
									class="w-full rounded-md px-2 py-1 text-left text-xs text-foreground/60 hover:bg-accent hover:text-accent-foreground focus-visible:ring-2 focus-visible:ring-primary"
									onclick={() => (sessionScope = 'all')}
								>
									Show closed sessions
								</button>
							</li>
						{/if}
					</ul>
					{/if}
				</div>
				{/if}
			</div>
		</div>

		<div class="relative p-2">
			<button type="button" class={cn(menuButton, 'h-9 px-3 text-[13px] hover:bg-muted')} onclick={() => openView('settings')}><GearIcon /><span>Settings</span></button>
			<span class="absolute top-3.5 right-3 flex size-5 items-center justify-center"><QuestionIcon class="size-4" /></span>
		</div>
	</aside>

	{#if setup === 'sessions'}
		<section class="demo-scroll min-w-0 flex-1 overflow-auto p-4">
			<button type="button" class="mb-3 flex items-center gap-1 rounded-md p-1 text-sm hover:bg-muted" onclick={() => setup = null}><CaretLeftIcon />Back to chat</button>
			<p class="mb-4 text-xl font-semibold">Sessions</p>
			<Tabs.Root value="all">
				<Tabs.List variant="line" aria-label="Session views">
					<Tabs.Trigger value="all">All</Tabs.Trigger>
					<Tabs.Trigger value="pinned">Pinned</Tabs.Trigger>
				</Tabs.List>
				{#each ['all', 'pinned'] as view}
					<Tabs.Content value={view}>
						<ul class="divide-y divide-border">
							{#each (view === 'all' ? device.sessions : pinnedSessions) as item (item.id)}
								{@render sessionItem(item)}
							{/each}
						</ul>
					</Tabs.Content>
				{/each}
			</Tabs.Root>
		</section>
	{:else if setup}
		{#key setup}
			<DemoSetup {device} mode={setup} bind:section={settingsSection} onsavememory={(text) => device.memory = text} onback={() => setup = null} onstart={(next) => {
				device.sessions.unshift(next);
				workspaceFilter = '';
				sessionsOpen = true;
				panelOpen = false;
				selectSession(next);
			}} />
		{/key}
	{:else if bot}
		{#key bot.id}
			<DemoBotChat
				{bot}
				workspaces={device.workspaces}
				onopensidebar={() => (mobileSidebarOpen = true)}
				ontodochange={(entry, todo) => (entry.todo = todo)}
				onsend={(text, quote) => bot.entries.push({ id: crypto.randomUUID(), kind: 'user', text, quote, time: 'Just now' })}
			/>
		{/key}
	{:else}
		<div class="relative flex min-w-0 flex-1 flex-col">
			<!-- Like the Dashboard, the top bar floats over the conversation. -->
			<div class="pointer-events-none absolute inset-x-0 top-0 z-10 flex h-12 items-center gap-2 px-3 @3xl:px-4">
				<div class="pointer-events-auto flex h-8 max-md:h-10 min-w-0 items-center rounded-lg bg-popover px-1 ring-1 ring-foreground/10">
					<button type="button" class="flex size-10 shrink-0 items-center justify-center rounded-md hover:bg-muted @3xl:hidden" aria-label="Open sidebar" aria-expanded={mobileSidebarOpen} onclick={() => (mobileSidebarOpen = true)}>
						<CaretLeftIcon class="size-4" />
					</button>
					<span class="mx-1 hidden min-w-0 max-w-80 truncate px-2 py-0.5 text-sm text-heading @3xl:block">{session.title}</span>
				</div>
				<span class="flex-1"></span>
				<span class="pointer-events-auto flex h-8 max-md:h-10 shrink-0 items-center rounded-lg bg-popover px-1 ring-1 ring-foreground/10 @3xl:hidden" aria-hidden="true">
					<span class="flex size-7 items-center justify-center"><DotsThreeIcon class="size-5" weight="bold" /></span>
				</span>
				<span class="pointer-events-auto flex h-8 max-md:h-10 shrink-0 items-center rounded-lg bg-popover px-1 ring-1 ring-foreground/10">
					<button
						type="button"
						class={cn('relative flex size-7 max-md:size-10 items-center justify-center rounded-md hover:bg-muted focus-visible:ring-2 focus-visible:ring-primary', panelOpen && 'bg-muted text-heading')}
						bind:this={panelButton}
						aria-label="Sidebar"
						aria-pressed={panelOpen}
						title="Sidebar"
						onclick={() => (panelOpen = !panelOpen)}
					>
						<SidebarSimpleIcon class="size-4 -scale-x-100" />
					</button>
				</span>
			</div>
			{#key session.id}
				<DemoConversation {session} {workspace} onopenfile={openFile} />
			{/key}
		</div>

		{#if panelOpen}
			<DemoSessionPanel {session} {workspace} bind:tab={panelTab} bind:file bind:diff onopenfile={openFile} onclear={() => (session.recentFiles = [])} onclose={closePanel} />
		{/if}
	{/if}
</div>

<style>
	/* Git status colours from the Dashboard theme. The details popover renders outside the demo and sets the class too. */
	:global(.demo-git-colors) {
		--demo-git-added: #98971a;
		--demo-git-modified: #d79921;
		--demo-git-untracked: #689d6a;
		--demo-git-behind: #b16286;
	}
	:global(.dark .demo-git-colors) {
		--demo-git-added: #b8bb26;
		--demo-git-modified: #fabd2f;
		--demo-git-untracked: #8ec07c;
		--demo-git-behind: #d3869b;
	}
	/* The site styles focused buttons with an offset outline; the demo uses the Dashboard's focus ring. */
	.dashboard-demo :global(button:focus-visible) {
		outline: none;
	}
	/* The site sets `font: inherit` on buttons outside any layer, which overrides the demo's text size utilities. */
	.dashboard-demo :global(button) {
		font: revert-layer;
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
