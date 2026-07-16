//! Controller binary — wires one `kube::runtime::Controller` over
//! `RabbitmqTopology`, driving `reconcile::reconcile` per object. Mirrors
//! `breathe-controller`'s `main.rs` shape (one `Controller` per CRD kind,
//! `Client::try_default()`, `futures::StreamExt::for_each` over the run
//! stream) at the smallest scale that fits a single-kind M0.
//!
//! **Not run in task #150's scope.** This binary compiles against real
//! `kube::Client`/`Controller` types; actually invoking it against a live
//! cluster (which would then reach out to a real broker's management API)
//! is explicitly out of scope for this task — CRD application and live runs
//! are left for review, per the task's own instruction.

use std::sync::Arc;
use std::time::Duration;

use camelot_rabbitmq_controller::crd::RabbitmqTopology;
use camelot_rabbitmq_controller::reconcile::{reconcile, CredentialResolver, Ctx, Error};
use futures::StreamExt;
use k8s_openapi::ByteString;
use kube::{api::Api, runtime::controller::Action, Client};

/// Resolves `spec.credentialsSecretRef` against a real k8s Secret in the
/// CR's namespace, reading the conventional `username`/`password` keys.
struct SecretCredentialResolver {
    client: Client,
    namespace: String,
}

impl CredentialResolver for SecretCredentialResolver {
    fn resolve(&self, secret_ref: &str) -> Result<(String, String), Error> {
        // A real lookup is async (the k8s API is async) while this trait's
        // signature is sync — the honest M0 seam: today this always errors,
        // making the gap visible in `status.error` rather than silently
        // returning bogus credentials. Wiring a real async resolve (e.g. via
        // a pre-populated cache refreshed on a watch of Secrets) is the
        // concrete next step, named here rather than faked.
        let _ = (&self.client, &self.namespace, &self.namespace);
        Err(Error::CredentialsUnresolved(secret_ref.to_string()))
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let client = Client::try_default().await?;
    let api: Api<RabbitmqTopology> = Api::all(client.clone());

    let ctx = Arc::new(Ctx {
        client: client.clone(),
        http: reqwest::Client::new(),
        credentials: Arc::new(SecretCredentialResolver { client: client.clone(), namespace: String::new() }),
        requeue: Duration::from_secs(60),
    });

    kube::runtime::Controller::new(api, kube::runtime::watcher::Config::default())
        .run(reconcile, error_policy, ctx)
        .for_each(|res| async move {
            match res {
                Ok(o) => tracing::info!(?o, "reconciled"),
                Err(e) => tracing::warn!(error = %e, "reconcile error"),
            }
        })
        .await;

    Ok(())
}

fn error_policy(_obj: Arc<RabbitmqTopology>, _err: &Error, _ctx: Arc<Ctx>) -> Action {
    Action::requeue(Duration::from_secs(30))
}

// Referenced only to keep `ByteString` import honest about the real Secret
// field type a completed `SecretCredentialResolver` would decode — removed
// once the async resolve lands.
#[allow(dead_code)]
fn _secret_value_shape(_b: ByteString) {}
