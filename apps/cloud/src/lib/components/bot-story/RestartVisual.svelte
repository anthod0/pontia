<script lang="ts">
	import HardDrivesIcon from 'phosphor-svelte/lib/HardDrivesIcon';
	import PlugsConnectedIcon from 'phosphor-svelte/lib/PlugsConnectedIcon';
	import ArrowsClockwiseIcon from 'phosphor-svelte/lib/ArrowsClockwiseIcon';
	import AgentBadge from './AgentBadge.svelte';
	import BotAvatarMark from './BotAvatarMark.svelte';
	import { claude, codex, pi, type Story } from '$lib/bot-stories';
	import { m } from '$lib/paraglide/messages';

	let { story, step }: { story: Story; step: number } = $props();

	// Steps: 0 all connected, 1 pontiad restarts while agents keep working, 2 pontiad is back, 3 reconnected.
	const agents = [
		{ agent: codex, title: m.story_task_ui },
		{ agent: claude, title: m.story_task_api },
		{ agent: pi, title: m.story_task_tests }
	];
	const restarting = $derived(step === 1 || step === 2);
	const linked = $derived(step === 0 || step === 3);
	const progress = $derived([30, 50, 70, 88][step]);
	const daemonState = $derived(step === 1 ? 'restarting' : step === 3 ? 'reconnected' : 'running');
	const daemonText = $derived(
		{ running: m.story_restart_running, restarting: m.story_restart_restarting, reconnected: m.story_restart_reconnected }[daemonState]
	);
</script>

<div class="restart" style:--bot={story.color} style:--progress="{progress}%">
	<span class="who"><span class="who-avatar"><BotAvatarMark shape={story.shape} color={story.color} /></span>{story.name}</span>
	<div class={['daemon card', restarting && 'restarting']}>
		<span class="avatar"><HardDrivesIcon size={30} /></span>
		<span class="daemon-name mono">pontiad</span>
		<span class={['daemon-state', daemonState]}>
			{#if daemonState === 'restarting'}<ArrowsClockwiseIcon size={12} weight="bold" class="spin" />{:else}<PlugsConnectedIcon size={12} weight="bold" />{/if}
			{daemonText()}
		</span>
	</div>

	<ul class="agents">
		{#each agents as a, i (a.agent.client)}
			<li class={['link', linked && 'on', step === 3 && 'again']} style:--i={i}>
				<span class="wire"></span>
				<div class="agent card">
					<AgentBadge agent={a.agent} />
					<span class="title">{a.title()}</span>
					<span class="track"><span class="fill"></span></span>
					<span class="state running"><span class="dot"></span>{m.story_state_running()}</span>
				</div>
			</li>
		{/each}
	</ul>
</div>

<style>
	.restart { position: relative; display: grid; grid-template-columns: 220px minmax(0, 1fr); align-items: center; gap: 0; min-height: 364px; }
	.who { position: absolute; top: 0; left: 0; display: inline-flex; align-items: center; gap: 8px; font-size: 13px; font-weight: 500; color: var(--heading); }
	.who-avatar { width: 22px; height: 22px; }
	.daemon { position: relative; z-index: 1; display: grid; justify-items: center; gap: 8px; padding: 22px 16px; transition: border-color 300ms, opacity 300ms; }
	.daemon.restarting { border-style: dashed; border-color: var(--border-stronger); }
	.avatar { display: grid; place-items: center; width: 52px; height: 52px; border-radius: 12px; background: color-mix(in srgb, var(--bot) 14%, transparent); color: var(--bot); transition: opacity 300ms; }
	.restarting .avatar { opacity: 0.4; }
	.daemon-name { font-size: 13px; color: var(--heading); }
	.daemon-state { display: inline-flex; align-items: center; gap: 5px; font: 11px var(--font-mono); }
	.daemon-state.running, .daemon-state.reconnected { color: var(--coverage-supported); }
	.daemon-state.restarting { color: var(--color-busy); }
	.daemon-state :global(.spin) { animation: spin 1.2s linear infinite; }

	.agents { display: grid; gap: 14px; margin: 0; padding: 0; list-style: none; }
	.link { display: grid; grid-template-columns: 56px minmax(0, 1fr); align-items: center; }
	/* The wire to pontiad goes dashed and faint while it restarts, then lights up again. */
	.wire { height: 0; border-top: 2px dashed var(--border-stronger); opacity: 0.6; transition: border-color 300ms, opacity 300ms; }
	.link.on .wire { border-top-style: solid; border-color: var(--bot); opacity: 0.75; }
	.link.again .wire { animation: relink 500ms var(--ease-out) both; animation-delay: calc(var(--i) * 140ms); }
	.agent { display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: 6px 14px; align-items: center; padding: 11px 14px; }
	.title { grid-column: 1; font-size: 13px; font-weight: 500; color: var(--heading); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	.agent .state { grid-column: 2; grid-row: 1 / 3; }
	.track { grid-column: 1 / 3; height: 4px; border-radius: 999px; background: var(--muted); overflow: hidden; }
	.fill { display: block; width: var(--progress); height: 100%; border-radius: inherit; background: var(--color-busy); transition: width 1600ms linear; }

	@keyframes spin { to { rotate: 360deg; } }
	@keyframes relink { from { opacity: 0; } }
	@media (prefers-reduced-motion: reduce) { .daemon-state :global(.spin), .link.again .wire { animation: none; } .fill { transition: none; } }

	@media (max-width: 760px) {
		.restart { grid-template-columns: 1fr; min-height: 0; padding-top: 34px; }
		.daemon { grid-template-columns: auto 1fr; grid-template-rows: auto auto; justify-items: start; gap: 2px 12px; padding: 12px 14px; }
		.avatar { grid-row: 1 / 3; width: 36px; height: 36px; }
		.agents { gap: 10px; padding-left: 20px; border-left: 2px solid transparent; }
		.link { grid-template-columns: 1fr; position: relative; }
		.wire { position: absolute; left: -20px; top: -10px; bottom: 50%; width: 20px; height: auto; border-top: 0; border-left: 2px dashed var(--border-stronger); border-bottom: 2px dashed var(--border-stronger); border-bottom-left-radius: 8px; }
		.link.on .wire { border-left-style: solid; border-bottom-style: solid; border-color: var(--bot); }
		.link:first-child .wire { top: -10px; }
		.agents { margin-left: 26px; padding-left: 20px; padding-top: 10px; }
		.agent { padding: 9px 12px; }
	}
</style>
