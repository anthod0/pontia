use std::{fs, path::Path};

#[test]
fn http_transport_does_not_depend_on_agent_clients() {
    let http_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/transport/http");
    let mut offenders = Vec::new();
    collect_agent_client_references(&http_dir, &mut offenders);

    assert!(
        offenders.is_empty(),
        "HTTP transport must not reference agent_clients directly; move capability interpretation into application: {}",
        offenders.join(", ")
    );
}

fn collect_agent_client_references(path: &Path, offenders: &mut Vec<String>) {
    if path.is_dir() {
        for entry in fs::read_dir(path).expect("read http transport dir") {
            let entry = entry.expect("read http transport entry");
            collect_agent_client_references(&entry.path(), offenders);
        }
        return;
    }

    if path.extension().and_then(|extension| extension.to_str()) != Some("rs") {
        return;
    }

    let contents = fs::read_to_string(path).expect("read http transport source file");
    if contents.contains("agent_clients") || contents.contains("pontia_agent_clients") {
        offenders.push(
            path.strip_prefix(env!("CARGO_MANIFEST_DIR"))
                .unwrap_or(path)
                .display()
                .to_string(),
        );
    }
}
