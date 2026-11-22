<script lang="ts">
	import DesktopIcon from 'phosphor-svelte/lib/DesktopIcon';
	import HardDrivesIcon from 'phosphor-svelte/lib/HardDrivesIcon';
	import LaptopIcon from 'phosphor-svelte/lib/LaptopIcon';
	import AgentBadge from './AgentBadge.svelte';
	import BotAvatarMark from './BotAvatarMark.svelte';
	import StoryState from './StoryState.svelte';
	import { claude, codex, pi, storyByName, type SessionState, type Story } from '$lib/bot-stories';
	import { m } from '$lib/paraglide/messages';

	let { story, step }: { story: Story; step: number } = $props();

	// The Dashboard switches devices one per step. Each device lists its own Bots and Sessions.
	const devices = [
		{ icon: DesktopIcon, name: m.story_device_desk, bots: ['Tova', 'Atlas', 'Mika'], sessions: [{ agent: codex, title: m.story_task_ui, state: 'running' as SessionState }, { agent: pi, title: m.story_task_tests, state: 'done' as SessionState }] },
		{ icon: LaptopIcon, name: m.story_device_laptop, bots: ['Remy', 'Pip'], sessions: [{ agent: claude, title: m.story_task_login, state: 'running' as SessionState }] },
		{ icon: HardDrivesIcon, name: m.story_device_server, bots: ['Juno', 'Sol'], sessions: [{ agent: claude, title: m.story_task_api, state: 'done' as SessionState }, { agent: codex, title: m.story_context_task, state: 'running' as SessionState }] }
	];
	const current = $derived(step);
	const device = $derived(devices[step]);
</script>

<div class="devices" style:--bot={story.color}>
	<div class="window card">
		<div class="titlebar"><i></i><i></i><i></i><span class="mono">Dashboard</span></div>
		<div class="layout">
			<ul class="switcher">
				{#each devices as d, i (i)}
					{@const Icon = d.icon}
					<li class={['device', i === current && 'active']}><Icon size={15} /><span>{d.name()}</span></li>
				{/each}
			</ul>
			{#key current}
				<div class="content enter">
					<span class="heading">{m.story_devices_bots()}</span>
					<ul class="bots">
						{#each device.bots as name (name)}
							{@const bot = storyByName(name)}
							<li><span class="avatar"><BotAvatarMark shape={bot.shape} color={bot.color} /></span>{bot.name}</li>
						{/each}
					</ul>
					<span class="heading">{m.story_devices_sessions()}</span>
					<ul class="sessions">
						{#each device.sessions as session (session.title())}
							<li class="card"><AgentBadge agent={session.agent} /><span class="title">{session.title()}</span><StoryState state={session.state} /></li>
						{/each}
					</ul>
				</div>
			{/key}
		</div>
	</div>
</div>

<style>
	.devices { display: grid; place-items: center; }
	.window { width: min(100%, 760px); overflow: hidden; border-radius: 12px; }
	.titlebar { display: flex; align-items: center; gap: 6px; height: 34px; padding-inline: 14px; border-bottom: 1px solid var(--border); }
	.titlebar i { width: 9px; height: 9px; border-radius: 50%; background: var(--border-strong); }
	.titlebar .mono { margin-left: 10px; }
	.layout { display: grid; grid-template-columns: 200px minmax(0, 1fr); min-height: 320px; }

	.switcher { display: grid; align-content: start; gap: 4px; margin: 0; padding: 14px 10px; border-right: 1px solid var(--border); background: var(--surface); list-style: none; }
	.device { display: flex; align-items: center; gap: 8px; padding: 8px 10px; border-radius: 8px; font-size: 13px; color: var(--muted-foreground); transition: background-color 250ms, color 250ms; }
	.device span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	.device.active { background: color-mix(in srgb, var(--bot) 16%, transparent); color: var(--heading); font-weight: 500; }

	.content { display: grid; align-content: start; gap: 10px; padding: 18px 22px; }
	.heading { font: 11px var(--font-mono); color: var(--muted-foreground); text-transform: uppercase; letter-spacing: 0.04em; }
	.bots { display: flex; flex-wrap: wrap; gap: 8px 18px; margin: 0 0 8px; padding: 0; list-style: none; }
	.bots li { display: inline-flex; align-items: center; gap: 8px; font-size: 13px; font-weight: 500; color: var(--heading); }
	.avatar { width: 26px; height: 26px; }
	.sessions { display: grid; gap: 8px; margin: 0; padding: 0; list-style: none; }
	.sessions li { display: grid; grid-template-columns: auto minmax(0, 1fr) auto; gap: 12px; align-items: center; padding: 10px 14px; }
	.title { font-size: 13px; font-weight: 500; color: var(--heading); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

	/* Narrow screens: the device switcher becomes a row of tabs above the content. */
	@media (max-width: 760px) {
		.layout { grid-template-columns: 1fr; min-height: 0; }
		.switcher { grid-template-columns: repeat(3, minmax(0, 1fr)); padding: 8px; border-right: 0; border-bottom: 1px solid var(--border); }
		.device { flex-direction: column; gap: 4px; padding: 6px 4px; font-size: 11px; text-align: center; }
		.device span { max-width: 100%; }
		.content { padding: 14px; }
		.sessions li { grid-template-columns: minmax(0, 1fr) auto; gap: 4px 10px; padding: 9px 12px; }
		.sessions li :global(.badge) { grid-column: 1 / 3; }
	}
</style>
