use std::{
    future::Future,
    io::Write,
    net::{Ipv4Addr, SocketAddr},
    time::Duration,
};

use anyhow::{Context, Result};
use futures_util::{StreamExt, stream::FuturesUnordered};
use hickory_resolver::{
    TokioResolver,
    config::{NameServerConfigGroup, ResolveHosts, ResolverConfig},
    name_server::TokioConnectionProvider,
};
use tokio::time::{Instant, MissedTickBehavior};

const PROPAGATION_TIMEOUT: Duration = Duration::from_secs(300);
const QUERY_TIMEOUT: Duration = Duration::from_secs(5);
const RETRY_DELAY: Duration = Duration::from_secs(5);
const PROGRESS_INTERVAL: Duration = Duration::from_secs(15);

pub(crate) async fn wait_for_txt(
    hostname: &str,
    value: &str,
    output: &mut (impl Write + ?Sized),
) -> Result<()> {
    wait_for_txt_with_resolvers(hostname, value, &challenge_resolvers(), output).await
}

async fn wait_for_txt_with_resolvers(
    hostname: &str,
    value: &str,
    resolvers: &[TokioResolver],
    output: &mut (impl Write + ?Sized),
) -> Result<()> {
    let name = format!("_acme-challenge.{hostname}.");
    wait_for_txt_with_lookup(&name, || lookup_txt(resolvers, &name, value), output).await
}

fn challenge_resolvers() -> [TokioResolver; 4] {
    [
        Ipv4Addr::new(223, 5, 5, 5),
        Ipv4Addr::new(119, 29, 29, 29),
        Ipv4Addr::new(1, 1, 1, 1),
        Ipv4Addr::new(8, 8, 8, 8),
    ]
    .map(|ip| challenge_resolver(SocketAddr::from((ip, 53))))
}

fn challenge_resolver(address: SocketAddr) -> TokioResolver {
    // A host stub can cache NXDOMAIN longer than the propagation deadline.
    let servers = NameServerConfigGroup::from_ips_clear(&[address.ip()], address.port(), true);
    let config = ResolverConfig::from_parts(None, vec![], servers);
    let mut builder =
        TokioResolver::builder_with_config(config, TokioConnectionProvider::default());
    builder.options_mut().use_hosts_file = ResolveHosts::Never;
    builder.options_mut().cache_size = 0;
    builder.build()
}

async fn lookup_txt(resolvers: &[TokioResolver], name: &str, expected: &str) -> Result<bool> {
    let mut queries: FuturesUnordered<_> = resolvers.iter().map(|r| r.txt_lookup(name)).collect();
    let mut last_error = None;
    let mut received_records = false;
    while let Some(result) = queries.next().await {
        match result {
            Ok(records) => {
                if records.iter().any(|record| {
                    record
                        .txt_data()
                        .iter()
                        .flat_map(|part| part.iter().copied())
                        .eq(expected.bytes())
                }) {
                    return Ok(true);
                }
                received_records = true;
            }
            Err(error) => last_error = Some(error),
        }
    }
    if !received_records && let Some(error) = last_error {
        return Err(error.into());
    }
    Ok(false)
}

async fn wait_for_txt_with_lookup<F, Fut>(
    name: &str,
    mut lookup: F,
    output: &mut (impl Write + ?Sized),
) -> Result<()>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<bool>>,
{
    progress(
        output,
        &format!("Waiting for DNS-01 TXT record {name} using public DNS (up to 5 minutes)..."),
    )?;
    let started = Instant::now();
    let deadline = started + PROPAGATION_TIMEOUT;
    let mut reports = tokio::time::interval_at(started + PROGRESS_INTERVAL, PROGRESS_INTERVAL);
    reports.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut last_observation = "no DNS response received".to_owned();
    let mut next_query = started;
    loop {
        let query_at = next_query;
        let query = async {
            tokio::time::sleep_until(query_at).await;
            tokio::time::timeout(QUERY_TIMEOUT, lookup()).await
        };
        tokio::pin!(query);
        // Keep progress and the hard deadline independent of stalled DNS requests.
        let result = loop {
            tokio::select! {
                biased;
                _ = tokio::time::sleep_until(deadline) => {
                    anyhow::bail!("DNS-01 TXT record {name} did not propagate within 5 minutes; last check: {last_observation}");
                }
                result = &mut query => break result,
                _ = reports.tick() => {
                    progress(output, &format!("Still waiting for DNS-01 TXT after {} seconds; last check: {last_observation}.", started.elapsed().as_secs()))?;
                }
            }
        };
        match result {
            Ok(Ok(true)) => {
                progress(
                    output,
                    "DNS-01 TXT record is visible; starting Let's Encrypt validation...",
                )?;
                return Ok(());
            }
            Ok(Ok(false)) => {
                last_observation = "the expected TXT value is not visible yet".to_owned()
            }
            Ok(Err(error)) => last_observation = format!("public DNS query failed: {error:#}"),
            Err(_) => last_observation = "public DNS query timed out after 5 seconds".to_owned(),
        }
        next_query = Instant::now() + RETRY_DELAY;
    }
}

pub(crate) fn progress(output: &mut (impl Write + ?Sized), message: &str) -> Result<()> {
    writeln!(output, "{message}").context("failed to write certificate progress")?;
    output
        .flush()
        .context("failed to flush certificate progress")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn challenge_queries_use_public_nameservers_without_local_caching() {
        let resolvers = challenge_resolvers();
        for (resolver, expected) in resolvers.iter().zip([
            "223.5.5.5:53",
            "119.29.29.29:53",
            "1.1.1.1:53",
            "8.8.8.8:53",
        ]) {
            let expected: SocketAddr = expected.parse().unwrap();
            let servers = resolver.config().name_servers();
            assert!(!servers.is_empty());
            assert!(servers.iter().all(|server| server.socket_addr == expected));
            assert_eq!(resolver.options().cache_size, 0);
            assert_eq!(resolver.options().use_hosts_file, ResolveHosts::Never);
        }
    }

    async fn answer_txt(socket: &tokio::net::UdpSocket, parts: Option<&[&str]>) {
        use hickory_resolver::proto::{
            op::{Message, MessageType, ResponseCode},
            rr::{RData, Record, RecordType, rdata::TXT},
        };
        let mut bytes = [0u8; 4096];
        let (length, peer) = socket.recv_from(&mut bytes).await.unwrap();
        let request = Message::from_vec(&bytes[..length]).unwrap();
        let query = request.queries()[0].clone();
        assert_eq!(query.name().to_utf8(), "_acme-challenge.edge.example.");
        assert_eq!(query.query_type(), RecordType::TXT);
        let mut response = Message::new();
        response
            .set_id(request.id())
            .set_message_type(MessageType::Response)
            .set_recursion_desired(true)
            .set_recursion_available(true)
            .add_query(query.clone());
        if let Some(parts) = parts {
            response.add_answer(Record::from_rdata(
                query.name().clone(),
                60,
                RData::TXT(TXT::new(parts.iter().map(|s| s.to_string()).collect())),
            ));
        } else {
            response.set_response_code(ResponseCode::NXDomain);
        }
        socket
            .send_to(&response.to_vec().unwrap(), peer)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn accepts_matching_segments_despite_nxdomain_stale_and_silent_servers() {
        use tokio::net::UdpSocket;
        let silent = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let stale = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let negative = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let matching = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let resolvers = [&silent, &negative, &stale, &matching]
            .map(|s| challenge_resolver(s.local_addr().unwrap()));
        let serve = async {
            answer_txt(&negative, None).await;
            answer_txt(&stale, Some(&["old-value"])).await;
            silent.recv_from(&mut [0; 512]).await.unwrap();
            answer_txt(&matching, Some(&["expected-", "value"])).await;
        };
        let mut output = Vec::new();
        let query =
            wait_for_txt_with_resolvers("edge.example", "expected-value", &resolvers, &mut output);
        let (_, result) =
            tokio::time::timeout(Duration::from_secs(3), async { tokio::join!(serve, query) })
                .await
                .unwrap();
        result.unwrap();
    }

    #[tokio::test]
    async fn retries_uncached_after_stale_txt() {
        let socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let resolvers = [challenge_resolver(socket.local_addr().unwrap())];
        for (value, visible) in [("stale", false), ("expected", true)] {
            let query = lookup_txt(&resolvers, "_acme-challenge.edge.example.", "expected");
            let (_, result) = tokio::time::timeout(Duration::from_secs(3), async {
                let parts = [value];
                tokio::join!(answer_txt(&socket, Some(&parts)), query)
            })
            .await
            .unwrap();
            assert_eq!(result.unwrap(), visible);
        }
    }

    #[tokio::test(start_paused = true)]
    async fn stops_at_five_minutes_and_reports_every_fifteen_seconds_even_with_stalled_dns() {
        let started = Instant::now();
        let mut output = Vec::new();
        let error = wait_for_txt_with_lookup(
            "_acme-challenge.edge.example.",
            std::future::pending::<Result<bool>>,
            &mut output,
        )
        .await
        .unwrap_err();
        assert_eq!(started.elapsed(), PROPAGATION_TIMEOUT);
        let output = String::from_utf8(output).unwrap();
        for seconds in (15..300).step_by(15) {
            assert!(
                output.contains(&format!("after {seconds} seconds")),
                "{output}"
            );
        }
        assert!(error.to_string().contains("timed out after 5 seconds"));
    }

    #[tokio::test(start_paused = true)]
    async fn rejects_stale_txt_and_reports_last_dns_failure() {
        let mut calls = 0;
        let error = wait_for_txt_with_lookup(
            "_acme-challenge.edge.example.",
            || {
                calls += 1;
                std::future::ready(if calls == 1 {
                    Ok(false)
                } else {
                    Err(anyhow::anyhow!("resolver unavailable"))
                })
            },
            &mut Vec::new(),
        )
        .await
        .unwrap_err();
        assert!(calls > 1);
        assert!(error.to_string().contains("resolver unavailable"));
    }

    #[tokio::test(start_paused = true)]
    async fn continues_when_expected_value_becomes_visible() {
        let mut calls = 0;
        let mut output = Vec::new();
        wait_for_txt_with_lookup(
            "_acme-challenge.edge.example.",
            || {
                calls += 1;
                std::future::ready(Ok(calls >= 3))
            },
            &mut output,
        )
        .await
        .unwrap();
        assert_eq!(calls, 3);
        assert!(
            String::from_utf8(output)
                .unwrap()
                .contains("starting Let's Encrypt validation")
        );
    }
}
