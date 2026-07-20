import type { SessionView, TurnView } from '../../api/types';

export interface CurrentTurnOutputSelection {
  turn: TurnView;
  title: 'Current branch turn output' | 'Latest turn output';
  outputSummary: string | null;
}

function turnTime(turn: TurnView): string {
  return turn.created_at || turn.started_at || turn.completed_at || '';
}

export function selectCurrentTurnOutput(
  session: Pick<SessionView, 'current_turn_id'>,
  turns: TurnView[],
): CurrentTurnOutputSelection | null {
  if (!turns.length) return null;

  if (session.current_turn_id) {
    const currentTurn = turns.find((turn) => turn.turn_id === session.current_turn_id);
    if (currentTurn) {
      return {
        turn: currentTurn,
        title: 'Current branch turn output',
        outputSummary: currentTurn.output?.summary ?? null,
      };
    }
  }

  const latestTurn = [...turns].sort((a, b) => turnTime(b).localeCompare(turnTime(a)))[0];
  return {
    turn: latestTurn,
    title: 'Latest turn output',
    outputSummary: latestTurn.output?.summary ?? null,
  };
}
