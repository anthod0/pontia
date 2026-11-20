-- Local reference vocabulary only. The run bootstrap seeds the test user and
-- public edge; tickets and devices are created through the application flows.
INSERT INTO hero_name_modifiers (word) VALUES ('brave'), ('calm'), ('silent')
ON CONFLICT (word) DO NOTHING;
INSERT INTO hero_name_heroes (word) VALUES ('arthur'), ('atlas'), ('orion')
ON CONFLICT (word) DO NOTHING;
