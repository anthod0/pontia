import type { SessionView } from '../api/types';

export function isTerminalSession(session: Pick<SessionView, 'state'>): boolean {
  return session.state === 'exited' || session.state === 'error';
}

export function sessionStateDotClass(state: string): string {
  switch (state) {
    case 'busy':
      return 'bg-success';
    case 'starting':
    case 'idle':
      return 'bg-warning';
    case 'interrupted':
      return 'bg-interrupted';
    case 'error':
      return 'bg-destructive';
    default:
      return 'bg-muted-foreground';
  }
}
