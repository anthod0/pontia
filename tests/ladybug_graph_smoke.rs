#![cfg(feature = "kuzu")]

#[test]
fn ladybug_database_writes_queries_and_reopens() {
    let temp_dir = tempfile::tempdir().expect("temp graph db dir");
    let db_path = temp_dir.path().join("ladybug-smoke");

    {
        let db = kuzu::Database::new(
            db_path.clone(),
            kuzu::SystemConfig::default().enable_multi_writes(true),
        )
        .expect("open fresh Ladybug database");
        let conn = kuzu::Connection::new(&db).expect("connect fresh Ladybug database");
        conn.query("CREATE NODE TABLE IF NOT EXISTS Smoke(id STRING, value STRING, PRIMARY KEY(id));")
            .expect("create smoke table");
        conn.query("MERGE (s:Smoke {id: 'smoke-1'}) SET s.value = 'created';")
            .expect("write smoke node");

        let mut result = conn
            .query("MATCH (s:Smoke {id: 'smoke-1'}) RETURN s.value;")
            .expect("query smoke node");
        assert_eq!(
            result.next().expect("smoke row")[0],
            kuzu::Value::String("created".to_string())
        );
    }

    {
        let db = kuzu::Database::new(
            db_path,
            kuzu::SystemConfig::default().enable_multi_writes(true),
        )
        .expect("reopen Ladybug database");
        let conn = kuzu::Connection::new(&db).expect("connect reopened Ladybug database");
        let mut result = conn
            .query("MATCH (s:Smoke {id: 'smoke-1'}) RETURN s.value;")
            .expect("query reopened smoke node");
        assert_eq!(
            result.next().expect("persisted smoke row")[0],
            kuzu::Value::String("created".to_string())
        );
    }
}
