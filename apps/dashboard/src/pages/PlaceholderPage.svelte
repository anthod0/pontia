<script lang="ts">
  import * as Card from '$lib/components/ui/card/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'

  let {
    title,
    eyebrow = 'Dashboard v2',
    description,
    details = [],
  }: {
    title: string
    eyebrow?: string
    description: string
    details?: string[]
  } = $props()
</script>

<section class="space-y-6">
  <div class="space-y-2">
    <p class="text-sm font-medium text-muted-foreground">{eyebrow}</p>
    <h2 class="text-3xl font-semibold tracking-tight">{title}</h2>
    <p class="max-w-3xl text-muted-foreground">{description}</p>
  </div>

  <Card.Root>
    <Card.Content class="pt-6">
      <Empty.Root>
        <Empty.Header>
          <Empty.Title>{title} placeholder</Empty.Title>
          <Empty.Description>
            This route is wired for browser-history navigation and will be connected to External API data in later phases.
          </Empty.Description>
        </Empty.Header>
        {#if details.length}
          <Empty.Content>
            <ul class="list-disc space-y-1 pl-5 text-left text-sm text-muted-foreground">
              {#each details as detail}
                <li>{detail}</li>
              {/each}
            </ul>
          </Empty.Content>
        {/if}
      </Empty.Root>
    </Card.Content>
  </Card.Root>
</section>
