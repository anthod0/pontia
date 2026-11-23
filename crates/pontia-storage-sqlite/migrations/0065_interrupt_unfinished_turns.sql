UPDATE turns SET state = 'interrupted' WHERE state = 'abandoned';
UPDATE workflow_turn_snapshots SET state = 'interrupted' WHERE state = 'abandoned';
