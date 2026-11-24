<script module lang="ts">
	export const settingsSections = ['Common', 'Agent Clients', 'Agent Profiles', 'Workspaces', 'Memory'] as const;
	export type DemoSettingsSection = (typeof settingsSections)[number];
</script>

<script lang="ts">
	import ArrowLeftIcon from 'phosphor-svelte/lib/ArrowLeftIcon';
	import { Button } from '$lib/components/ui/button';
	import { demoClients, type DemoClient, type DemoDevice, type DemoSession } from './mockData';

	let { device, mode, section = $bindable(), onback, onstart, onsavememory }: {
		device: DemoDevice;
		mode: 'settings' | 'new';
		section: DemoSettingsSection;
		onback: () => void;
		onsavememory: (text: string) => void;
		onstart: (session: DemoSession) => void;
	} = $props();

	let detail = $state<DemoClient | null>(null);
	let memoryDraft = $state<string | null>(null);
	let workspaceId = $state('');
	let client = $state<DemoClient>('codex');
	let choice = $state(0);
	let prompt = $state('');
	const modelChoices = $derived([
		{ model: 'Client default', reasoningEffort: undefined },
		...demoClients[client].models.flatMap(model =>
			['low', 'medium', 'high'].map(reasoningEffort => ({ model, reasoningEffort })))
	]);
	const field = 'min-h-10 w-full rounded-md border border-border bg-background px-2 text-sm';

	function start() {
		const id = crypto.randomUUID();
		const preset = modelChoices[choice];
		onstart({
			id, title: prompt.trim(), workspaceId: workspaceId || device.workspaces[0].id,
			client, model: preset.model, reasoningEffort: preset.reasoningEffort,
			handle: id.slice(0, 8), state: 'busy', prompt: prompt.trim(),
			duration: 0, context: '0%', steps: [], reply: []
		});
	}
</script>

<section class="demo-scroll flex min-w-0 flex-1 flex-col overflow-auto px-4 pb-4">
	<header class="flex min-h-12 items-center gap-2 text-sm font-medium">
		<Button variant="ghost" size="icon" aria-label="Back to chat" onclick={onback}><ArrowLeftIcon /></Button>
		{mode === 'new' ? 'New Chat' : 'Settings'}
	</header>
	{#if mode === 'new'}
		<div class="mx-auto flex w-full max-w-2xl flex-1 flex-col py-4">
			<div class="my-auto py-3">
				<img src="/logo.svg" alt="" class="mx-auto mb-3 size-14" />
				<p class="text-center text-2xl font-semibold text-heading">Start a session</p>
				<p class="mt-1 text-center text-sm text-muted-foreground">Choose a workspace and agent client, then describe the job.</p>
			</div>
			<form class="grid gap-3" onsubmit={(event) => { event.preventDefault(); start(); }}>
				<label class="grid gap-1 text-xs">Workspace
					<select aria-label="Workspace" class={field} value={workspaceId || device.workspaces[0].id} onchange={(event) => workspaceId = event.currentTarget.value}>
						{#each device.workspaces as item}<option value={item.id}>{item.name}</option>{/each}
					</select>
				</label>
				<div class="grid gap-3 @md:grid-cols-2">
					<label class="grid gap-1 text-xs">Agent Client
						<select aria-label="Agent Client" class={field} bind:value={client} onchange={() => choice = 0}>
							{#each device.clients as key}<option value={key}>{demoClients[key].name}</option>{/each}
						</select>
					</label>
					<label class="grid gap-1 text-xs">Model
						<select aria-label="Model" class={field} bind:value={choice}>
							{#each modelChoices as preset, index}<option value={index}>{preset.model}{preset.reasoningEffort ? ' · ' + preset.reasoningEffort : ''}</option>{/each}
						</select>
					</label>
				</div>
				<textarea class="rounded-md border border-border bg-background p-2 text-sm" aria-label="Message" placeholder="Describe the job…" bind:value={prompt}></textarea>
				<Button type="submit" class="justify-self-end" disabled={!prompt.trim()}>Send</Button>
			</form>
		</div>
	{:else}
		<div class="mx-auto w-full max-w-xl py-4">
			<p class="hidden text-xl font-semibold @3xl:block">{section}</p>
			<label class="grid gap-1 text-xs @3xl:hidden">Settings section
				<select aria-label="Settings section" class={field} bind:value={section}>{#each settingsSections as name}<option>{name}</option>{/each}</select>
			</label>
			<ul class="mt-4 divide-y divide-border text-sm">
				{#if section === 'Common'}
					<li class="py-3"><strong>Devices</strong><p>{device.name} · Online</p><code class="text-xs text-muted-foreground">{device.handle}</code></li>
				{:else if section === 'Memory'}
					<li class="grid gap-2 py-3">
						<strong>USER.md</strong>
						{#if memoryDraft === null}
							<p class="whitespace-pre-wrap">{device.memory}</p>
							<Button variant="outline" size="sm" class="justify-self-start" onclick={() => memoryDraft = device.memory}>Edit</Button>
						{:else}
							<textarea aria-label="USER.md" rows="4" class="rounded-md border border-border bg-background p-2" bind:value={memoryDraft}></textarea>
							<div class="flex gap-2">
								<Button size="sm" onclick={() => { onsavememory(memoryDraft!); memoryDraft = null; }}>Save</Button>
								<Button size="sm" variant="outline" onclick={() => memoryDraft = null}>Cancel</Button>
							</div>
						{/if}
					</li>
				{:else if section === 'Workspaces'}
					{#each device.workspaces as item}<li class="py-3"><strong>{item.name}</strong><p class="break-all text-xs text-muted-foreground">{item.path}</p></li>{/each}
				{:else if section === 'Agent Clients'}
					{#each device.clients as key}
						<li>
							<button type="button" class="flex min-h-11 w-full items-center gap-3 rounded-md px-2 text-left hover:bg-muted" aria-expanded={detail === key} onclick={() => detail = detail === key ? null : key}>
								<img src={'/client-logos/' + demoClients[key].logo + '.svg'} alt="" class="size-5" />
								{demoClients[key].name}
							</button>
							{#if detail === key}
								<div class="px-2 pb-3 text-xs text-muted-foreground">
									{#if key === 'pi'}<p>Version 0.70.2</p>{/if}
									<p>Models · {demoClients[key].models.join(', ') || 'Client default'}</p>
								</div>
							{/if}
						</li>
					{/each}
				{:else}
					<li class="py-3"><strong>Coder</strong><p class="text-xs text-muted-foreground">Codex · gpt-6.2-astra · medium</p></li>
					<li class="py-3"><strong>Designer</strong><p class="text-xs text-muted-foreground">Claude Code · opus · high</p></li>
				{/if}
			</ul>
		</div>
	{/if}
</section>
