<script lang="ts">
	import FolderSimpleIcon from 'phosphor-svelte/lib/FolderSimpleIcon';
	import GitBranchIcon from 'phosphor-svelte/lib/GitBranchIcon';
	import GitMergeIcon from 'phosphor-svelte/lib/GitMergeIcon';
	import AgentBadge from './AgentBadge.svelte';
	import BotAvatarMark from './BotAvatarMark.svelte';
	import StoryState from './StoryState.svelte';
	import { claude, codex, type Story } from '$lib/bot-stories';
	import { m } from '$lib/paraglide/messages';

	let { story, step }: { story: Story; step: number } = $props();

	// Two branches fork from main, each worked on in its own worktree, then merge back.
	const branches = [
		{ y: 20, branch: 'agent/settings-ui', path: '~/worktrees/settings-ui', agent: codex, task: m.story_task_ui, from: 1, doneAt: 4 },
		{ y: 80, branch: 'agent/login-redirect', path: '~/worktrees/login-redirect', agent: claude, task: m.story_task_login, from: 2, doneAt: 3 }
	];
	const merged = $derived(step >= 4);
	// Desktop and narrow layouts place the forks at different x positions.
	const layouts = [
		{ name: 'wide', fork: 12, start: 24, end: 76, merge: 88 },
		{ name: 'narrow', fork: 4, start: 14, end: 86, merge: 96 }
	];
	const curve = (a: number, b: number, from: number, to: number) => {
		const mid = (a + b) / 2;
		return `M${a} ${from}C${mid} ${from} ${mid} ${to} ${b} ${to}`;
	};
</script>

<div class="worktrees" style:--bot={story.color}>
	<span class="who"><span class="avatar"><BotAvatarMark shape={story.shape} color={story.color} /></span>{story.name}</span>

	{#each layouts as layout (layout.name)}
		<svg class={['graph', layout.name]} viewBox="0 0 100 100" preserveAspectRatio="none">
			<path class="main-line" d="M0 50H100" />
			{#each branches as b (b.branch)}
				<path class={['branch-line', step >= b.from && 'on']} d="{curve(layout.fork, layout.start, 50, b.y)}H{layout.end}" />
				<path class={['branch-line', merged && 'on']} d={curve(layout.end, layout.merge, b.y, 50)} />
			{/each}
		</svg>
	{/each}

	<span class="main-label mono"><GitBranchIcon size={12} />main</span>
	<span class={['merged', merged && 'on']}><GitMergeIcon size={13} weight="bold" />{m.story_worktrees_merged()}</span>

	{#each branches as b (b.branch)}
		<div class="slot" style:top="{b.y}%">
			{#if step >= b.from}
				<div class="card enter">
					<span class="branch mono"><GitBranchIcon size={12} />{b.branch}</span>
					<span class="path mono"><FolderSimpleIcon size={12} />{b.path}</span>
					<span class="task">{b.task()}</span>
					<span class="foot"><AgentBadge agent={b.agent} /><StoryState state={step >= b.doneAt ? 'done' : 'running'} /></span>
				</div>
			{/if}
		</div>
	{/each}
</div>

<style>
	.worktrees { position: relative; height: 364px; --start: 24%; --width: 52%; }
	.who { position: absolute; top: 0; left: 0; display: inline-flex; align-items: center; gap: 8px; font-size: 13px; font-weight: 500; color: var(--heading); }
	.avatar { width: 22px; height: 22px; }

	.graph { position: absolute; inset: 0; width: 100%; height: 100%; overflow: visible; }
	.graph.narrow { display: none; }
	.graph path { fill: none; stroke-width: 2; vector-effect: non-scaling-stroke; stroke-linecap: round; }
	.main-line { stroke: var(--border-stronger); }
	.branch-line { stroke: var(--bot); opacity: 0; transition: opacity 400ms; }
	.branch-line.on { opacity: 0.7; }

	.main-label { position: absolute; left: 0; top: 50%; translate: 0 calc(-100% - 6px); display: inline-flex; align-items: center; gap: 5px; }
	.merged { position: absolute; right: 0; top: 50%; translate: 0 8px; display: inline-flex; align-items: center; gap: 5px; font: 11px var(--font-mono); color: var(--coverage-supported); opacity: 0; transition: opacity 400ms; }
	.merged.on { opacity: 1; }

	.slot { position: absolute; left: var(--start); width: var(--width); translate: 0 -50%; }
	.card { display: grid; gap: 5px; padding: 11px 14px; }
	.mono { display: inline-flex; align-items: center; gap: 5px; overflow: hidden; white-space: nowrap; }
	.branch { color: var(--heading); }
	.task { font-size: 13px; font-weight: 500; color: var(--heading); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	.foot { display: flex; align-items: center; justify-content: space-between; gap: 10px; }

	@media (max-width: 760px) {
		.worktrees { height: 400px; --start: 14%; --width: 72%; }
		.graph.wide { display: none; }
		.graph.narrow { display: block; }
		.who { top: auto; bottom: 0; }
		/* The forks sit at the edges here, so both labels move to the gap between the cards. */
		.main-label { left: 50%; translate: -50% calc(-100% - 6px); }
		.merged { right: auto; left: 50%; translate: -50% 8px; }
		.path { display: none; }
		.card { padding: 9px 12px; }
	}
</style>
