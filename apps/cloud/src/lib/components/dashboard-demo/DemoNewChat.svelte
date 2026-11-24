<script lang="ts">
	import ArrowLeftIcon from 'phosphor-svelte/lib/ArrowLeftIcon';
	import { Button } from '$lib/components/ui/button';
	import { demoClients, type DemoClient, type DemoDevice, type DemoSession } from './mockData';

	let { device, onback, onstart }: {
		device: DemoDevice;
		onback: () => void;
		onstart: (session: DemoSession) => void;
	} = $props();

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
		New Chat
	</header>
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
</section>
