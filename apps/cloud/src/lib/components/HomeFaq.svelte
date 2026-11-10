<script lang="ts">
	import PlusIcon from 'phosphor-svelte/lib/PlusIcon';
	import { reveal } from '$lib/reveal';

	// `answer` is plain text for structured data; `links` render after it on the page.
	const faqs: { question: string; answer: string; links?: { href: string; label: string }[] }[] = [
		{
			question: 'Do my agents run in Pontia Cloud?',
			answer: 'No. Agent Clients, tools, credentials, and project files stay on your device. Pontia Cloud handles sign-in, device registration, and connection authorization.',
			links: [{ href: '/docs/how-it-works', label: 'How Pontia works' }]
		},
		{
			question: 'Can Pontia read my prompts or code?',
			answer: 'Dashboard traffic is encrypted end to end between your browser and your device. The Edge relays ciphertext, and Pontia Cloud is not in the traffic path.',
			links: [{ href: '/docs/security', label: 'Security' }]
		},
		{
			question: 'Which coding agents does Pontia support?',
			answer: 'Pontia integrates with pi, Codex, Claude Code, Grok Build, and OpenCode. Each agent keeps its own login, models, and sandbox, and the available controls depend on the client.',
			links: [{ href: '/docs/agent-clients', label: 'Agent Clients' }]
		},
		{
			question: 'Which platforms are supported?',
			answer: 'Pontia runs on x86_64 and ARM64 Linux and on macOS 11 or later. The Dashboard works in modern desktop and mobile browsers and can be installed to a phone home screen.',
			links: [{ href: '/docs/getting-started', label: 'Getting started' }]
		},
		{
			question: 'How much does Pontia cost?',
			answer: 'The hosted service currently has no usage fee. You keep paying your model providers directly through your own agent accounts.'
		}
	];

	const structuredData = {
		'@context': 'https://schema.org',
		'@type': 'FAQPage',
		mainEntity: faqs.map((faq) => ({
			'@type': 'Question',
			name: faq.question,
			acceptedAnswer: { '@type': 'Answer', text: faq.answer }
		}))
	};
</script>

<svelte:head>
	{@html `<script type="application/ld+json">${JSON.stringify(structuredData)}</script>`}
</svelte:head>

<section id="faq" class="faq page-width section-block" aria-labelledby="faq-title" {@attach reveal()}>
	<div class="section-intro reveal">
		<div class="section-heading">
			<h2 id="faq-title">Questions, answered.</h2>
		</div>
		<a class="faq-more" href="/docs/faq">Full FAQ <span aria-hidden="true">↗</span></a>
	</div>
	<div class="faq-list">
		{#each faqs as faq, index}
			<details class="reveal" style={`--i: ${index + 1}`}>
				<summary><h3>{faq.question}</h3><PlusIcon size={16} aria-hidden="true" /></summary>
				<p>
					{faq.answer}
					{#each faq.links ?? [] as link}<a href={link.href}>{link.label}</a>{/each}
				</p>
			</details>
		{/each}
	</div>
</section>

<style>
	/* Heading stays beside the questions on wide screens. */
	.faq { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1.6fr); align-items: start; gap: 48px; padding-top: 56px; }
	.faq .section-intro { position: sticky; top: 48px; flex-direction: column; align-items: flex-start; gap: 20px; margin: 0; }
	.faq-more { padding-bottom: 6px; color: var(--primary); font-size: 13px; font-weight: 500; }
	.faq-more:hover { text-decoration: underline; }
	.faq-list { border-top: 1px solid var(--border); }
	details { border-bottom: 1px solid var(--border); }
	summary { display: flex; align-items: center; justify-content: space-between; gap: 16px; padding: 20px 0; list-style: none; cursor: pointer; color: var(--muted-foreground); }
	summary::-webkit-details-marker { display: none; }
	summary:hover h3 { color: var(--primary); }
	summary:focus-visible { outline: 2px solid var(--primary); outline-offset: 4px; }
	summary :global(svg) { flex-shrink: 0; transition: transform 0.3s var(--ease-out); }
	details[open] summary :global(svg) { transform: rotate(45deg); color: var(--primary); }
	/* Animate the answer height where the browser supports it. */
	details::details-content { block-size: 0; overflow: hidden; transition: block-size 0.35s var(--ease-out), content-visibility 0.35s allow-discrete; }
	details[open]::details-content { block-size: auto; }
	.faq { interpolate-size: allow-keywords; }
	h3 { font-size: 16px; font-weight: 500; letter-spacing: -0.015em; }
	details p { max-width: 760px; padding-bottom: 22px; font-size: 14px; line-height: 1.8; color: var(--muted-foreground); }
	details p a { margin-left: 8px; color: var(--primary); white-space: nowrap; }
	details p a:hover { text-decoration: underline; }
	@media (max-width: 900px) {
		.faq { grid-template-columns: 1fr; gap: 0; }
		.faq .section-intro { position: static; flex-direction: row; align-items: flex-end; justify-content: space-between; width: 100%; margin-bottom: 27px; }
	}
	@media (max-width: 600px) {
		.faq { padding-top: 24px; }
		.faq .section-intro { flex-direction: column; align-items: flex-start; }
		h3 { font-size: 15px; }
	}
</style>
