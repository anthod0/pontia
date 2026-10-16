<script lang="ts">
  import * as Table from '$lib/components/ui/table';
  import CheckIcon from 'phosphor-svelte/lib/CheckIcon';
  import CircleHalfIcon from 'phosphor-svelte/lib/CircleHalfIcon';
  import XIcon from 'phosphor-svelte/lib/XIcon';
  import MinusIcon from 'phosphor-svelte/lib/MinusIcon';
  import CircleDashedIcon from 'phosphor-svelte/lib/CircleDashedIcon';

  // SVG logo sources: https://pi.dev/press-kit; Claude symbol extracted from
  // the inline wordmark at https://claude.com/product/claude-code;
  // https://grok.com/images/favicon.svg; OpenCode brand assets at
  // https://opencode.ai/brand. The OpenAI symbol used for Codex comes from
  // anomalyco/opencode: packages/ui/src/assets/icons/provider/openai.svg.
  // Local vector assets live in static/client-logos/.
  // Coverage checked against client-{pi,codex,acp}/src/{spec,adapter}.rs,
  // Codex service/interactions.rs, and pontia-config/src/acp.rs.
  const features = ['Live conversation', 'Two-way sync', 'Approvals', 'History branching', 'Context management', 'Model switching'];
  const states = {
    supported: { label: 'Supported', icon: CheckIcon },
    partial: { label: 'Partial', icon: CircleHalfIcon },
    unsupported: { label: 'Not supported', icon: XIcon },
    notApplicable: { label: 'N/A', icon: MinusIcon },
    planning: { label: 'Planning', icon: CircleDashedIcon },
  };
  // Not supported is reserved for confirmed technical limitations, not missing work.
  // ACP TUI sync and history branching remain planned investigations; their
  // feasibility and scope depend on the individual agent, not ACP alone.
  type Coverage = keyof typeof states;
  const clients: { name: string; logo: string; coverage: Coverage[] }[] = [
    { name: 'Pi', logo: '/client-logos/pi.svg', coverage: ['supported', 'supported', 'notApplicable', 'supported', 'supported', 'supported'] },
    { name: 'Codex', logo: '/client-logos/codex.svg', coverage: ['supported', 'partial', 'supported', 'planning', 'partial', 'supported'] },
    { name: 'Claude Code', logo: '/client-logos/claude-code.svg', coverage: ['planning', 'planning', 'planning', 'planning', 'planning', 'planning'] },
    { name: 'Grok Build', logo: '/client-logos/grok-build.svg', coverage: ['supported', 'planning', 'supported', 'planning', 'planning', 'planning'] },
    { name: 'OpenCode', logo: '/client-logos/opencode.svg', coverage: ['supported', 'planning', 'supported', 'planning', 'planning', 'planning'] },
  ];
</script>

<section id="progress" class="integrations page-width section-block" aria-labelledby="integrations-title">
  <div class="section-intro">
    <div>
      <h2 id="integrations-title">Your clients.<br />Coming together.</h2>
    </div>
  </div>
  <div class="coverage-table">
    <Table.Root>
      <Table.Caption class="sr-only">Pontia client integration coverage. Supported indicates implemented integration, not a guarantee for every client configuration.</Table.Caption>
      <Table.Header>
        <Table.Row>
          <Table.Head class="client-heading">Agent client</Table.Head>
          {#each features as feature}<Table.Head class="feature-heading">{feature}</Table.Head>{/each}
        </Table.Row>
      </Table.Header>
      <Table.Body>
        {#each clients as client}
          <Table.Row>
            <Table.Head scope="row" class="client-cell">
              <span class="client-identity"><img class="client-mark" src={client.logo} alt="" width="32" height="32" loading="lazy" /><span>{client.name}</span></span>
            </Table.Head>
            {#each client.coverage as status}
              {@const Icon = states[status].icon}
              <Table.Cell class="coverage-cell"><span class={`coverage-status ${status}`} title={states[status].label}><Icon size={19} weight="regular" aria-hidden="true" /><span class="sr-only">{states[status].label}</span></span></Table.Cell>
            {/each}
          </Table.Row>
        {/each}
      </Table.Body>
    </Table.Root>
  </div>
  <ul class="coverage-legend" aria-label="Capability status legend">
    {#each Object.entries(states) as [status, state]}
      {@const Icon = state.icon}
      <li><span class={`coverage-status ${status}`}><Icon size={17} weight="regular" aria-hidden="true" /></span><span>{state.label}</span></li>
    {/each}
  </ul>
  <div class="section-footer"><p>One workspace. More ways to work.</p><a href="/docs/agent-clients">Explore client integrations <span aria-hidden="true">↗</span></a></div>
</section>

<style>
  .integrations { padding-top: 56px; }
  .coverage-table { border-top: 1px solid var(--border); }
  .coverage-table :global(table) { min-width: 980px; table-layout: fixed; }
  .coverage-table :global(.client-heading) { width: 190px; padding: 16px 12px 16px 0; }
  .coverage-table :global(.feature-heading) { padding: 16px 10px; text-align: center; white-space: normal; font-size: 12px; line-height: 1.6; }
  .coverage-table :global(.client-cell) { padding: 22px 12px 22px 0; color: var(--heading); }
  .client-identity { display: flex; align-items: center; gap: 14px; font-size: 15px; font-weight: 500; white-space: nowrap; }
  .client-mark { display: block; flex-shrink: 0; width: 32px; height: 32px; object-fit: contain; }
  .coverage-table :global(.coverage-cell) { padding: 22px 10px; text-align: center; }
  .coverage-status { display: inline-flex; align-items: center; justify-content: center; vertical-align: middle; color: var(--muted-foreground); }
  .coverage-legend { display: flex; flex-wrap: wrap; gap: 12px 24px; list-style: none; padding: 20px 0 0; margin: 0; }
  .coverage-legend li { display: flex; align-items: center; gap: 7px; font-size: 11px; color: var(--muted-foreground); }
  .coverage-status.supported { color: #326b60; }
  .coverage-status.partial { color: #8a651e; }
  .section-footer { display: flex; justify-content: space-between; gap: 16px; padding-top: 24px; font-size: 12px; }
  .section-footer p { color: var(--muted-foreground); }
  .section-footer a { color: var(--primary); }
  .section-footer a:hover { text-decoration: underline; }
  @media(max-width: 600px) {
    .integrations { padding-top: 24px; }
    .coverage-table :global(.client-heading), .coverage-table :global(.client-cell) { position: sticky; left: 0; background: var(--background); z-index: 1; padding-left: 8px; width: 165px; }
    .client-identity { gap: 10px; font-size: 14px; }
    .section-footer { flex-direction: column; gap: 10px; }
  }
</style>
