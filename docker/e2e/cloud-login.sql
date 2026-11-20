-- The test user has no provider account; the browser starts with this session.
INSERT INTO users (id, display_name)
VALUES ('{user_id}', 'E2E test user')
ON CONFLICT (id) DO NOTHING;

INSERT INTO auth_sessions (id, user_id, kind, token_hash, expires_at)
VALUES ('{session_id}', '{user_id}', 'browser', '{token_hash}', '{expires_at}')
ON CONFLICT (id) DO NOTHING;
