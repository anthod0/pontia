<script lang="ts">
  interface Props {
    count: number
    class?: string
    onOpen: () => void
  }

  let { count: _count, class: _className, onOpen: _onOpen }: Props = $props()
</script>
