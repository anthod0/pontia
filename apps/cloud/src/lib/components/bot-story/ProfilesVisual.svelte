<script lang="ts">
	import AgentBadge from './AgentBadge.svelte';
	import BotAvatarMark from './BotAvatarMark.svelte';
	import StoryState from './StoryState.svelte';
	import { acp, claude, codex, pi, type SessionState, type Story } from '$lib/bot-stories';
	import { m } from '$lib/paraglide/messages';

	let { story, step }: { story: Story; step: number } = $props();

	// One user request fans out to four helpers, each started from a different Agent Profile.
	const lanes = [
		{ profile: m.story_profile_ui, agent: codex, task: m.story_task_ui, doneAt: 4 },
		{ profile: m.story_profile_review, agent: claude, task: m.story_task_api, doneAt: 2 },
		{ profile: m.story_profile_docs, agent: pi, task: m.story_task_docs, doneAt: 3 },
		{ profile: m.story_profile_research, agent: acp, task: m.story_task_research, doneAt: 3 }
	];
	const state = (doneAt: number): SessionState => (step >= doneAt ? 'done' : 'running');
</script>

<div class="profiles" style:--bot={story.color}>
	<div class="hub">
		<span class="msg user enter">{m.story_profiles_ask()}</span>
		<span class={['avatar', step >= 1 && step < 4 && 'working']}><BotAvatarMark shape={story.shape} color={story.color} /></span>
		<span class="name">{story.name}</span>
	</div>

	<ul class={['lanes', step >= 1 && 'open']}>
		{#each lanes as lane, i (lane.agent.client)}
			<li class="lane" style:--i={i}>
				{#if step >= 1}
					<div class="card enter" style:animation-delay="{i * 120}ms">
						<span class="profile">{lane.profile()}</span>
						<AgentBadge agent={lane.agent} />
						<span class="task">{lane.task()}</span>
						<StoryState state={state(lane.doneAt)} />
					</div>
				{/if}
			</li>
		{/each}
	</ul>
</div>

<style>
	.profiles { display: grid; grid-template-columns: minmax(0, 0.8fr) minmax(0, 1.2fr); gap: 0; align-items: center; }
	.hub { display: flex; flex-direction: column; align-items: center; gap: 14px; padding-right: 28px; }
	.hub .msg { align-self: center; max-width: 100%; text-align: center; }
	.avatar { width: 72px; height: 72px; }
	.working { animation: bob 1.6s ease-in-out infinite; }
	.name { font-size: 13px; font-weight: 500; color: var(--heading); }

	/* Connectors: a vertical spine on the left of the lanes and one branch per lane. */
	.lanes { position: relative; display: grid; gap: 12px; margin: 0; padding: 0 0 0 36px; list-style: none; }
	.lanes::before { content: ''; position: absolute; left: 0; top: 37px; bottom: 37px; width: 2px; background: var(--bot); opacity: 0; transition: opacity 300ms; }
	.lanes.open::before { opacity: 0.45; }
	.lane { position: relative; min-height: 74px; }
	.lane::before { content: ''; position: absolute; left: -36px; top: 50%; width: 36px; height: 2px; background: var(--bot); opacity: 0; transform-origin: left; transition: opacity 300ms; }
	.open .lane::before { opacity: 0.45; }
	.card { display: grid; grid-template-columns: auto 1fr auto; gap: 6px 12px; align-items: center; padding: 11px 14px; }
	.profile { padding: 2px 8px; border-radius: 999px; background: color-mix(in srgb, var(--bot) 16%, transparent); font-size: 11px; font-weight: 500; color: var(--heading); white-space: nowrap; }
	.card :global(.badge) { grid-column: 2 / 4; justify-self: start; }
	.task { grid-column: 1 / 3; font-size: 13px; font-weight: 500; color: var(--heading); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

	@keyframes bob { 50% { translate: 0 -3px; } }
	@media (prefers-reduced-motion: reduce) { .working { animation: none; } }

	@media (max-width: 760px) {
		.profiles { grid-template-columns: 1fr; gap: 14px; }
		.hub { flex-direction: row; justify-content: flex-start; flex-wrap: wrap; padding: 0; gap: 10px; }
		.hub .msg { order: 2; flex-basis: 100%; align-self: flex-end; text-align: left; max-width: 86%; margin-left: auto; flex-basis: auto; }
		.avatar { width: 32px; height: 32px; }
		.lanes { gap: 8px; padding-left: 22px; }
		.lanes::before { left: 0; top: 33px; bottom: 33px; }
		.lane { min-height: 66px; }
		.lane::before { left: -22px; width: 22px; }
		.card { padding: 9px 12px; }
	}
</style>
