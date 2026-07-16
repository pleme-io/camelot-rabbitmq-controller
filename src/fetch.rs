//! The one I/O seam: calling the live RabbitMQ management HTTP API. Kept
//! deliberately thin and untested-by-unit-test (there is no live broker in
//! CI for this M0) — every fact this crate reasons about is pulled out into
//! `observed.rs`'s pure, unit-tested `diff_topology`. Promoting this to a
//! mockable `Environment` trait (per the TYPED-SPEC + INTERPRETER TRIPLET
//! discipline) is the natural M1 move once a write path exists to fake too.

use crate::observed::{ObservedQueue, ObservedVhost, ObservedTopology};

#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error("http request to the RabbitMQ management API failed: {0}")]
    Request(#[from] reqwest::Error),
}

/// Fetch the live broker's full vhost + queue state via the management API's
/// two list endpoints. `base_url` is the CR's `spec.managementApi`;
/// `(user, pass)` are read from the k8s Secret named by
/// `spec.credentialsSecretRef` (resolved by the caller — this fn takes the
/// credential value, never the secret name, so it never has to know how
/// secrets are materialized).
pub async fn fetch_topology(
    client: &reqwest::Client,
    base_url: &str,
    user: &str,
    pass: &str,
) -> Result<ObservedTopology, FetchError> {
    let vhosts: Vec<ObservedVhost> = client
        .get(format!("{base_url}/api/vhosts"))
        .basic_auth(user, Some(pass))
        .send()
        .await?
        .json()
        .await?;

    let queues: Vec<ObservedQueue> = client
        .get(format!("{base_url}/api/queues"))
        .basic_auth(user, Some(pass))
        .send()
        .await?
        .json()
        .await?;

    Ok(ObservedTopology { vhosts, queues })
}
