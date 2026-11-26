mod support;

use reqwest::StatusCode;
use support::TestEdge;

const ROBOTS_POLICY: &str = "noindex, nofollow, noarchive, nosnippet";

#[tokio::test]
async fn prevents_indexing_of_successful_and_not_found_responses() {
    let server = TestEdge::start().await;

    for (path, status) in [
        ("/healthz", StatusCode::OK),
        ("/missing", StatusCode::NOT_FOUND),
    ] {
        let response = server
            .http
            .get(format!("{}{path}", server.edge_origin))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), status);
        assert_eq!(response.headers()["x-robots-tag"], ROBOTS_POLICY);
    }
}
