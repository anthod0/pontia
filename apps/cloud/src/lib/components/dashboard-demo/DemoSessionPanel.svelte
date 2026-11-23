<script lang="ts">
	import ArrowClockwiseIcon from 'phosphor-svelte/lib/ArrowClockwiseIcon';
	import FileTextIcon from 'phosphor-svelte/lib/FileTextIcon';
	import FolderIcon from 'phosphor-svelte/lib/FolderIcon';
	import GitBranchIcon from 'phosphor-svelte/lib/GitBranchIcon';
	import XIcon from 'phosphor-svelte/lib/XIcon';
	import * as Tabs from '$lib/components/ui/tabs/index.js';
	import DemoFilesTab from './DemoFilesTab.svelte';
	import DemoGitChanges from './DemoGitChanges.svelte';
	import type { DemoChange, DemoSession, DemoWorkspace } from './mockData';

	let { session, workspace, tab = $bindable(), file = $bindable(), diff = $bindable(), onopenfile, onclear, onclose }: {
		session: DemoSession;
		workspace: DemoWorkspace;
		tab: 'files' | 'git';
		file: string | null;
		diff: DemoChange | null;
		onopenfile: (path: string) => void;
		onclear: () => void;
		onclose: () => void;
	} = $props();
	let changes = $state<DemoGitChanges>();
	const tabTrigger = 'h-full px-2.5 text-sm aria-selected:text-foreground after:inset-x-0 after:-bottom-px after:h-0.5 aria-selected:after:opacity-100';
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<aside tabindex="-1" {@attach node => node.focus({ preventScroll: true })} onkeydown={(event) => changes?.keydown(event)} aria-label="Sidebar"
	class="absolute inset-0 z-20 flex min-w-0 flex-col bg-surface outline-none @5xl:static @5xl:z-auto @5xl:w-[28rem] @5xl:shrink-0 @5xl:border-l @5xl:border-border">
	<Tabs.Root bind:value={tab} class="min-h-0 flex-1 flex-col gap-0">
		<header class="flex h-12 shrink-0 items-center justify-between gap-1 border-b border-border px-2">
			<Tabs.List variant="line" aria-label="Sidebar" class="h-full gap-0 p-0">
				<Tabs.Trigger value="files" class={tabTrigger}><FileTextIcon />Files</Tabs.Trigger>
				<Tabs.Trigger value="git" class={tabTrigger}><GitBranchIcon />Git</Tabs.Trigger>
			</Tabs.List>
			<button type="button" class="flex size-7 items-center justify-center rounded-md hover:bg-muted focus-visible:ring-2 focus-visible:ring-primary" aria-label="Close" title="Close" onclick={onclose}><XIcon class="size-4" /></button>
		</header>

		<DemoFilesTab files={session.files ?? {}} {file} recent={session.recentFiles ?? []} description="Files you open from this chat show here." onopen={onopenfile} onclose={() => (file = null)} {onclear} />

		<Tabs.Content value="git" class="flex min-h-0 flex-col">
			{#key workspace.id}
				<DemoGitChanges bind:this={changes} git={workspace.git} bind:file={diff} hint="Alt G toggle · ↑↓ select · Enter open">
					{#snippet header()}
						<div class="flex h-12 shrink-0 items-center gap-2 border-b border-border pr-2 pl-4">
							<FolderIcon class="size-4 shrink-0 text-muted-foreground" />
							<span class="min-w-0 flex-1 truncate text-sm font-medium text-heading" title={workspace.path}>{workspace.name}</span>
							<span class="text-xs text-muted-foreground tabular-nums">{workspace.git.changes.length}</span>
							<span class="flex size-7 items-center justify-center" aria-hidden="true"><ArrowClockwiseIcon class="size-4" /></span>
						</div>
					{/snippet}
				</DemoGitChanges>
			{/key}
		</Tabs.Content>
	</Tabs.Root>
</aside>
