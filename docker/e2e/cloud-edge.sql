-- The public edge owner has no login account; the test user can select this edge.
INSERT INTO users (id, display_name)
VALUES ('{owner_id}', 'E2E edge operator')
ON CONFLICT (id) DO NOTHING;

INSERT INTO edges (id, user_id, access_scope, name, tunnel_url, service_credential_hash, preview_enabled)
VALUES ('{edge_id}', '{owner_id}', 'public', '{name}',
        'wss://{hostname}/tunnel', '{credential_hash}', 1)
ON CONFLICT (id) DO NOTHING;
