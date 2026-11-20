DELETE FROM execution_profiles
WHERE json_extract(metadata, '$.builtin') = 1;
