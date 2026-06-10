import { getContext, setContext } from 'svelte';

const CHAIN_OF_THOUGHT_CONTEXT = Symbol('chain-of-thought');

export class ChainOfThoughtContext {
  open = $state(false);

  constructor(defaultOpen = false) {
    this.open = defaultOpen;
  }

  toggle(): void {
    this.open = !this.open;
  }
}

export function setChainOfThoughtContext(defaultOpen: () => boolean = () => false): ChainOfThoughtContext {
  return setContext(CHAIN_OF_THOUGHT_CONTEXT, new ChainOfThoughtContext(defaultOpen()));
}

export function getChainOfThoughtContext(): ChainOfThoughtContext {
  const context = getContext<ChainOfThoughtContext>(CHAIN_OF_THOUGHT_CONTEXT);
  if (!context) throw new Error('ChainOfThought components must be used within ChainOfThought.Root');
  return context;
}
