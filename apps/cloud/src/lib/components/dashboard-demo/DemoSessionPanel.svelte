<script lang="ts">
	import FileTextIcon from 'phosphor-svelte/lib/FileTextIcon';
	import FolderIcon from 'phosphor-svelte/lib/FolderIcon';
	import GitBranchIcon from 'phosphor-svelte/lib/GitBranchIcon';
	import GlobeIcon from 'phosphor-svelte/lib/GlobeIcon';
	import XIcon from 'phosphor-svelte/lib/XIcon';
	import * as Tabs from '$lib/components/ui/tabs/index.js';
	import DemoFilesTab from './DemoFilesTab.svelte';
	import DemoGitChanges from './DemoGitChanges.svelte';
	import DemoPreviewTab from './DemoPreviewTab.svelte';
	import DemoStackHeader from './DemoStackHeader.svelte';
	import type { DemoChange, DemoSession, DemoWorkspace } from './mockData';

	let { session, workspace, tab = $bindable(), file = $bindable(), diff = $bindable(), preview = $bindable(), onopenfile, onclear, onclose }: {
		session: DemoSession;
		workspace: DemoWorkspace;
		tab: 'files' | 'preview' | 'git';
		file: string | null;
		diff: DemoChange | null;
		/** The localhost address the Preview tab shows. */
		preview: string | null;
		onopenfile: (path: string) => void;
		onclear: () => void;
		onclose: () => void;
	} = $props();
	let changes = $state<DemoGitChanges>();
	const tabTrigger = 'h-full px-2.5 text-sm aria-selected:text-foreground after:inset-x-0 after:-bottom-px after:h-0.5 aria-selected:after:opacity-100';
	// On the phone each tab is a pushed screen, titled like the mobile Dashboard's.
	const stack = $derived(
		tab === 'git'
			? (changes?.title() ?? { title: 'Git', subtitle: session.title })
			: tab === 'files' && file
				? { title: `${workspace.path}/${file}`, subtitle: undefined }
				: { title: tab === 'files' ? 'Files' : 'Preview', subtitle: session.title }
	);

	function back() {
		if (tab === 'git' && changes?.goBack()) return;
		if (tab === 'files' && file) file = null;
		else onclose();
	}
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<aside tabindex="-1" {@attach node => node.focus({ preventScroll: true })} onkeydown={(event) => {
		if (event.key !== 'Escape' || event.defaultPrevented) return;
		event.preventDefault();
		back();
	}} aria-label="Sidebar"
	class="demo-stack-screen absolute inset-0 z-30 flex min-w-0 flex-col bg-surface outline-none @5xl:static @5xl:z-auto @5xl:w-[28rem] @5xl:shrink-0 @5xl:border-l @5xl:border-border">
	<div class="hidden @max-3xl/demo:block"><DemoStackHeader title={stack.title} subtitle={stack.subtitle} onback={back} /></div>
	<Tabs.Root bind:value={tab} class="min-h-0 flex-1 flex-col gap-0">
		<header class="flex h-12 shrink-0 items-center justify-between gap-1 border-b border-border px-2 @max-3xl/demo:hidden">
			<Tabs.List variant="line" aria-label="Sidebar" class="h-full gap-0 p-0">
				<Tabs.Trigger value="files" class={tabTrigger}><FileTextIcon />Files</Tabs.Trigger>
				<Tabs.Trigger value="preview" class={tabTrigger}><GlobeIcon />Preview</Tabs.Trigger>
				<Tabs.Trigger value="git" class={tabTrigger}><GitBranchIcon />Git</Tabs.Trigger>
			</Tabs.List>
			<button type="button" class="flex size-7 items-center justify-center rounded-full hover:bg-muted focus-visible:ring-2 focus-visible:ring-primary" aria-label="Close" title="Close" onclick={onclose}><XIcon class="size-4" /></button>
		</header>

		<DemoFilesTab files={session.files ?? {}} {file} recent={session.recentFiles ?? []} description="Files you open from this chat show here." onopen={onopenfile} onclose={() => (file = null)} {onclear} />

		{#if tab === 'preview'}<DemoPreviewTab bind:url={preview} />{/if}

		<Tabs.Content value="git" class="flex min-h-0 flex-col">
			{#key workspace.id}
				<DemoGitChanges bind:this={changes} git={workspace.git} bind:file={diff} hint="Alt G toggle · ↑↓ select · Enter open">
					{#snippet heading()}
						<FolderIcon class="size-4 shrink-0 text-muted-foreground" />
						<span class="min-w-0 flex-1 truncate text-sm font-medium text-heading" title={workspace.path}>{workspace.name}</span>
					{/snippet}
				</DemoGitChanges>
			{/key}
		</Tabs.Content>
	</Tabs.Root>
</aside>
