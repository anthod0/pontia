<script lang="ts">
	import FileMagnifyingGlassIcon from 'phosphor-svelte/lib/FileMagnifyingGlassIcon';
	import PencilSimpleIcon from 'phosphor-svelte/lib/PencilSimpleIcon';
	import SpinnerGapIcon from 'phosphor-svelte/lib/SpinnerGapIcon';
	import TerminalWindowIcon from 'phosphor-svelte/lib/TerminalWindowIcon';
	import AgentBadge from './AgentBadge.svelte';
	import BotAvatarMark from './BotAvatarMark.svelte';
	import StoryState from './StoryState.svelte';
	import { claude, codex, pi, type Story } from '$lib/bot-stories';
	import { m } from '$lib/paraglide/messages';

	let { story, step }: { story: Story; step: number } = $props();

	// The Bot's helpers are listed under its chat. Step 1 opens the Codex helper, step 3 the user steps in.
	const helpers = [
		{ agent: codex, title: m.story_task_ui },
		{ agent: claude, title: m.story_task_api },
		{ agent: pi, title: m.story_task_tests }
	];
	const open = $derived(step >= 1);
</script>

<div class="sessions" style:--bot={story.color}>
	<div class="chat card">
		<div class="bar"><span class="avatar"><BotAvatarMark shape={story.shape} color={story.color} /></span>{story.name}</div>
		<div class="feed">
			<span class="msg user enter">{m.story_work_ask()}</span>
			<span class="msg bot enter">{m.story_work_reply()}</span>
			<ul class="helpers enter">
				{#each helpers as helper, i (i)}
					<li class={['helper card', open && i === 0 && 'active']}>
						<AgentBadge agent={helper.agent} />
						<span class="title">{helper.title()}</span>
						<StoryState state="running" />
					</li>
				{/each}
			</ul>
		</div>
	</div>

	<div class="session card">
		<div class="bar">
			{#if open}
				<span class="session-title enter">{m.story_task_ui()}</span>
				<span class="session-agent enter"><AgentBadge agent={codex} /></span>
			{:else}
				<span class="mono">{m.story_work_session()}</span>
			{/if}
		</div>
		<ol class="steps">
			{#if open}
				<li class="step enter">
					<span class="icon"><FileMagnifyingGlassIcon size={15} /></span>
					<span class="step-body">
						<span class="step-title">{m.story_work_read()}</span>
						<span class="mono">routes/settings/+page.svelte</span>
						<span class="mono">lib/api/settings.ts</span>
					</span>
				</li>
			{/if}
			{#if step >= 2}
				<li class="step enter">
					<span class="icon"><PencilSimpleIcon size={15} /></span>
					<span class="step-body">
						<span class="step-title">{m.story_work_edit()}</span>
						<span class="mono">routes/settings/+page.svelte <b class="added">+18</b> <b class="removed">−4</b></span>
					</span>
				</li>
				<li class="step enter" style:animation-delay="80ms">
					<span class="icon"><TerminalWindowIcon size={15} /></span>
					<span class="step-body">
						<span class="step-title">{m.story_work_run()}</span>
						<code class="terminal"><span>$ pnpm test settings</span><span class="output">✓ 12 passed</span></code>
					</span>
				</li>
			{/if}
			{#if step >= 3}
				<li class="msg user enter">{m.story_work_steer()}</li>
			{/if}
			{#if step >= 4}
				<li class="msg bot enter">{m.story_work_steer_reply()}</li>
				<li class="step enter" style:animation-delay="80ms">
					<span class="icon"><SpinnerGapIcon size={15} class="spin" /></span>
					<span class="step-body">
						<span class="step-title">{m.story_work_editing()}</span>
						<span class="mono">routes/settings/+page.svelte</span>
					</span>
				</li>
			{/if}
		</ol>
	</div>
</div>

<style>
	.sessions { display: grid; grid-template-columns: minmax(0, 0.9fr) minmax(0, 1.1fr); gap: 28px; align-items: center; }
	.chat, .session { display: flex; flex-direction: column; height: 364px; overflow: hidden; border-radius: 12px; }
	.bar { display: flex; align-items: center; gap: 10px; height: 42px; flex-shrink: 0; padding-inline: 14px; border-bottom: 1px solid var(--border); font-size: 13px; font-weight: 500; color: var(--heading); }
	.avatar { width: 22px; height: 22px; }
	.feed { display: flex; flex: 1; flex-direction: column; justify-content: flex-end; gap: 8px; padding: 14px; }

	.helpers { display: grid; gap: 6px; margin: 4px 0 0; padding: 0; list-style: none; }
	.helper { display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: 3px 10px; align-items: center; padding: 8px 12px; outline: 2px solid transparent; outline-offset: 2px; transition: outline-color 150ms; }
	.helper.active { outline-color: var(--primary); }
	.helper :global(.badge) { grid-column: 1; }
	.helper :global(.state) { grid-column: 2; grid-row: 1 / 3; }
	.title, .session-title { font-size: 13px; font-weight: 500; color: var(--heading); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	.session-agent { display: inline-flex; margin-left: auto; }

	/* The helper's Session: tool steps joined by a line, as in the Dashboard. Older steps scroll off the top. */
	.steps { display: flex; flex: 1; flex-direction: column; justify-content: flex-end; gap: 10px; min-height: 0; margin: 0; padding: 14px 16px; list-style: none; overflow: hidden; mask-image: linear-gradient(to bottom, transparent, #000 28px); }
	.step { position: relative; display: flex; flex-shrink: 0; gap: 10px; }
	.step + .step::before { content: ''; position: absolute; left: 11px; bottom: calc(100% + 2px); height: 10px; width: 1px; background: var(--border-strong); }
	.icon { display: grid; flex-shrink: 0; place-items: center; width: 22px; height: 22px; color: var(--muted-foreground); }
	.icon :global(.spin) { animation: spin 1s linear infinite; color: var(--color-busy); }
	.step-body { display: grid; flex: 1; gap: 3px; min-width: 0; padding-top: 2px; }
	.step-title { font-size: 13px; font-weight: 500; color: color-mix(in srgb, var(--foreground) 75%, transparent); }
	.step-body .mono { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	.added { font-weight: 500; color: var(--coverage-supported); }
	.removed { font-weight: 500; color: var(--destructive); }
	.terminal { display: grid; gap: 2px; margin-top: 3px; padding: 7px 10px; border-radius: 7px; background: var(--muted); font: 11px/1.5 var(--font-mono); color: var(--foreground); }
	.output { color: var(--coverage-supported); }
	.steps .msg { flex-shrink: 0; }
	.steps .msg.bot { margin-left: 32px; }

	@keyframes spin { to { rotate: 360deg; } }
	@media (prefers-reduced-motion: reduce) { .icon :global(.spin) { animation: none; } }

	@media (max-width: 760px) {
		.sessions { grid-template-columns: 1fr; gap: 14px; }
		.chat { height: auto; }
		.session { height: 280px; }
		.steps { padding: 12px; }
	}
</style>
