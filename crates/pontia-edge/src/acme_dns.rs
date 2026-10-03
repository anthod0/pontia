use std::{future::Future, io::Write, time::Duration};

use anyhow::{Context, Result};
use hickory_resolver::TokioResolver;
use tokio::time::{Instant, MissedTickBehavior};

use crate::network::system_dns_resolver;

const PROPAGATION_TIMEOUT: Duration = Duration::from_secs(300);
const QUERY_TIMEOUT: Duration = Duration::from_secs(5);
const RETRY_DELAY: Duration = Duration::from_secs(5);
const PROGRESS_INTERVAL: Duration = Duration::from_secs(15);

pub(crate) async fn wait_for_txt(
    hostname: &str,
    value: &str,
    output: &mut (impl Write + ?Sized),
) -> Result<()> {
    wait_for_txt_with_resolver(hostname, value, &system_dns_resolver()?, output).await
}

async fn wait_for_txt_with_resolver(
    hostname: &str,
    value: &str,
    resolver: &TokioResolver,
    output: &mut (impl Write + ?Sized),
) -> Result<()> {
    let name = format!("_acme-challenge.{hostname}.");
    wait_for_txt_with_lookup(
        &name,
        value,
        || async {
            let records = resolver.txt_lookup(name.as_str()).await?;
            Ok(records
                .iter()
                .map(|record| {
                    record
                        .txt_data()
                        .iter()
                        .flat_map(|part| part.iter().copied())
                        .collect()
                })
                .collect())
        },
        output,
    )
    .await
}

async fn wait_for_txt_with_lookup<F, Fut>(
    name: &str,
    expected: &str,
    mut lookup: F,
    output: &mut (impl Write + ?Sized),
) -> Result<()>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<Vec<Vec<u8>>>>,
{
    progress(
        output,
        &format!("Waiting for DNS-01 TXT record {name} using system DNS (up to 5 minutes)..."),
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
            Ok(Ok(records)) if records.iter().any(|record| record == expected.as_bytes()) => {
                progress(
                    output,
                    "DNS-01 TXT record is visible; starting Let's Encrypt validation...",
                )?;
                return Ok(());
            }
            Ok(Ok(_)) => last_observation = "the expected TXT value is not visible yet".to_owned(),
            Ok(Err(error)) => last_observation = format!("system DNS query failed: {error:#}"),
            Err(_) => last_observation = "system DNS query timed out after 5 seconds".to_owned(),
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
    async fn queries_configured_dns_and_concatenates_txt_character_strings() {
        use hickory_resolver::{
            config::{NameServerConfig, ResolverConfig},
            name_server::TokioConnectionProvider,
            proto::{
                op::{Message, MessageType},
                rr::{RData, Record, RecordType, rdata::TXT},
                xfer::Protocol,
            },
        };
        let socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let address = socket.local_addr().unwrap();
        let mut config = ResolverConfig::new();
        config.add_name_server(NameServerConfig::new(address, Protocol::Udp));
        let resolver =
            TokioResolver::builder_with_config(config, TokioConnectionProvider::default()).build();
        let server = tokio::spawn(async move {
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
                .add_query(query.clone())
                .add_answer(Record::from_rdata(
                    query.name().clone(),
                    60,
                    RData::TXT(TXT::new(vec!["expected-".to_owned(), "value".to_owned()])),
                ));
            socket
                .send_to(&response.to_vec().unwrap(), peer)
                .await
                .unwrap();
        });
        tokio::time::timeout(
            Duration::from_secs(3),
            wait_for_txt_with_resolver(
                "edge.example",
                "expected-value",
                &resolver,
                &mut Vec::new(),
            ),
        )
        .await
        .unwrap()
        .unwrap();
        server.await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn stops_at_five_minutes_and_reports_every_fifteen_seconds_even_with_stalled_dns() {
        let started = Instant::now();
        let mut output = Vec::new();
        let error = wait_for_txt_with_lookup(
            "_acme-challenge.edge.example.",
            "expected",
            std::future::pending::<Result<Vec<Vec<u8>>>>,
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
            "expected",
            || {
                calls += 1;
                std::future::ready(if calls == 1 {
                    Ok(vec![b"stale".to_vec()])
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
            "expected",
            || {
                calls += 1;
                std::future::ready(Ok(vec![if calls < 3 {
                    b"stale".to_vec()
                } else {
                    b"expected".to_vec()
                }]))
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
