export function sessionStateDotClass(state: string): string {
  switch (state) {
    case 'busy':
    case 'starting':
      return 'bg-amber-500';
    case 'idle':
    case 'interrupted':
      return 'bg-emerald-500';
    case 'error':
      return 'bg-destructive';
    default:
      return 'bg-muted-foreground';
  }
}
