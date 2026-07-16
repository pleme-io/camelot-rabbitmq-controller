//! The kube-rs reconcile loop, in the shape `breathe-controller` uses for
//! every band kind: `reconcile(obj: Arc<K>, ctx: Arc<Ctx>) -> Result<Action, Error>`,
//! status written back via a merge-patch, periodic re-drive via
//! `Action::requeue`. **M0 scope: observe-only.** This fn reads the live
//! broker and writes what it saw to `status` — it never calls a RabbitMQ
//! mutating endpoint. The write path (create/declare the missing
//! vhosts/queues) is explicitly out of scope here and is the honest M1.

use std::sync::Arc;
use std::time::Duration;

use kube::{
    api::{Api, Patch, PatchParams},
    runtime::controller::Action,
    Client, ResourceExt,
};
use serde_json::json;

use crate::crd::RabbitmqTopology;
use crate::fetch::{fetch_topology, FetchError};
use crate::observed::diff_topology;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("kube: {0}")]
    Kube(#[from] kube::Error),
    #[error("fetch: {0}")]
    Fetch(#[from] FetchError),
    #[error("credentials for secretRef '{0}' could not be resolved")]
    CredentialsUnresolved(String),
}

/// Shared reconcile context. `resolve_credentials` is a trait object so a
/// real k8s-Secret-backed resolver and a fixture resolver can both satisfy
/// it without the reconcile body caring which — the one seam this M0 does
/// make mockable, since "read a Secret" is the one dependency a test can't
/// otherwise avoid.
pub trait CredentialResolver: Send + Sync {
    fn resolve(&self, secret_ref: &str) -> Result<(String, String), Error>;
}

pub struct Ctx {
    pub client: Client,
    pub http: reqwest::Client,
    pub credentials: Arc<dyn CredentialResolver>,
    pub requeue: Duration,
}

pub async fn reconcile(obj: Arc<RabbitmqTopology>, ctx: Arc<Ctx>) -> Result<Action, Error> {
    let ns = obj.namespace().unwrap_or_default();
    let name = obj.name_any();
    let api: Api<RabbitmqTopology> = Api::namespaced(ctx.client.clone(), &ns);

    let (user, pass) = match ctx.credentials.resolve(&obj.spec.credentials_secret_ref) {
        Ok(creds) => creds,
        Err(e) => {
            patch_error(&api, &name, &e.to_string()).await?;
            return Ok(Action::requeue(ctx.requeue));
        }
    };

    let observed = match fetch_topology(&ctx.http, &obj.spec.management_api, &user, &pass).await {
        Ok(observed) => observed,
        Err(e) => {
            patch_error(&api, &name, &e.to_string()).await?;
            return Ok(Action::requeue(ctx.requeue));
        }
    };

    let drift = diff_topology(&obj.spec, &observed);
    tracing::info!(rabbitmq_topology = %name, drift_count = drift.len(), "observed live topology");

    let status = json!({
        "status": {
            "phase": "Observed",
            "driftCount": drift.len(),
            "drift": drift.iter().map(ToString::to_string).collect::<Vec<_>>(),
            "lastObservedAt": now_rfc3339(),
            "error": null,
        }
    });
    api.patch_status(&name, &PatchParams::apply("camelot-rabbitmq-controller"), &Patch::Merge(&status))
        .await?;

    Ok(Action::requeue(ctx.requeue))
}

async fn patch_error(api: &Api<RabbitmqTopology>, name: &str, message: &str) -> Result<(), kube::Error> {
    let status = json!({ "status": { "phase": "Error", "error": message } });
    api.patch_status(name, &PatchParams::apply("camelot-rabbitmq-controller"), &Patch::Merge(&status))
        .await?;
    Ok(())
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}
