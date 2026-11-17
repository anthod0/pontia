ALTER TABLE execution_profiles ADD COLUMN default_client_type TEXT;
ALTER TABLE execution_profiles ADD COLUMN default_client_key TEXT;
ALTER TABLE execution_profiles ADD COLUMN default_model TEXT
    CHECK (
        (default_client_type IS NULL AND default_client_key IS NULL AND default_model IS NULL)
        OR (default_client_type IS NOT NULL AND default_client_key IS NOT NULL AND default_model IS NOT NULL)
    );
