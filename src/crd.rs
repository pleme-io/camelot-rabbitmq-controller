//! The `RabbitmqTopology` CRD (`camelot.pleme.io/v1`) — the typed declaration
//! of a RabbitMQ broker's desired vhost/queue topology, per the M0 scope in
//! task #150: a real, compiling CRD + an observe-only reconciler. Modeled on
//! `breathe-crd`'s per-dimension `band_kind!` shape (one spec struct + a
//! `CustomResource` derive + a typed status carrying `Condition`s), scoped
//! down to exactly the fields this M0 needs — no `band_kind!`-style macro
//! yet (there is only one kind here, so a macro would be premature; if a
//! sibling kind — e.g. `RabbitmqPermission` — lands, extracting the shared
//! shape into a macro is the natural M1 move, same as breathe did).
//!
//! Not yet covered by this CRD (named honestly, not silently rounded up):
//! **permissions/users** (the actual task #145 gap — vhost/queue ACLs) and
//! **policies** (HA/mirroring, TTL). This M0 only models vhosts + queues,
//! the smallest real slice that lets the reconciler observe *something* true
//! against the live broker today.

use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A single declared queue inside a vhost.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct QueueSpec {
    pub name: String,
    #[serde(default = "d_true")]
    pub durable: bool,
}

fn d_true() -> bool {
    true
}

/// A single declared vhost and the queues that must exist inside it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct VhostSpec {
    pub name: String,
    #[serde(default)]
    pub queues: Vec<QueueSpec>,
}

/// `RabbitmqTopology.spec` — the desired state. `management_api` points at the
/// broker's management-plugin HTTP endpoint (e.g.
/// `http://rabbitmq.camelot.svc.cluster.local:15672`); `credentials_secret_ref`
/// names the k8s Secret holding the admin user/pass (never inlined here —
/// per ★★ cofre / zero-plaintext discipline, this CRD carries a reference,
/// not a value).
#[derive(CustomResource, Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[kube(
    group = "camelot.pleme.io",
    version = "v1",
    kind = "RabbitmqTopology",
    namespaced,
    status = "RabbitmqTopologyStatus",
    shortname = "rmqtopo",
    category = "camelot",
    printcolumn = r#"{"name":"Endpoint","type":"string","jsonPath":".spec.managementApi"}"#,
    printcolumn = r#"{"name":"Drifted","type":"integer","jsonPath":".status.driftCount"}"#,
    printcolumn = r#"{"name":"Phase","type":"string","jsonPath":".status.phase"}"#
)]
#[serde(rename_all = "camelCase")]
pub struct RabbitmqTopologySpec {
    pub management_api: String,
    pub credentials_secret_ref: String,
    #[serde(default)]
    pub vhosts: Vec<VhostSpec>,
}

/// `RabbitmqTopology.status` — the last-observed reconcile outcome.
/// M0 is observe-only: `phase` never reaches an "applied" state, only
/// `Observed` (drift computed) or `Error` (fetch/parse failed).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RabbitmqTopologyStatus {
    #[serde(default)]
    pub phase: String,
    #[serde(default)]
    pub drift_count: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub drift: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_observed_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use kube::CustomResourceExt;

    #[test]
    fn spec_round_trips_through_yaml() {
        let spec = RabbitmqTopologySpec {
            management_api: "http://rabbitmq.camelot.svc.cluster.local:15672".to_string(),
            credentials_secret_ref: "rabbitmq-admin-credentials".to_string(),
            vhosts: vec![VhostSpec {
                name: "example-app".to_string(),
                queues: vec![QueueSpec { name: "orders-events".to_string(), durable: true }],
            }],
        };
        let yaml = serde_yaml::to_string(&spec).expect("serialize");
        let parsed: RabbitmqTopologySpec = serde_yaml::from_str(&yaml).expect("deserialize");
        assert_eq!(parsed, spec);
    }

    #[test]
    fn queue_durable_defaults_true_when_omitted() {
        let parsed: QueueSpec = serde_yaml::from_str("name: q1\n").expect("deserialize");
        assert!(parsed.durable);
    }

    #[test]
    fn crd_definition_generates_without_panicking() {
        // Exercises the `CustomResource` derive's generated `crd()` — this is
        // exactly what `kubectl apply` would consume; asserting it builds a
        // valid CustomResourceDefinition object is the honest M0 proxy for
        // "this CRD is well-formed" without touching a live cluster.
        let crd = RabbitmqTopology::crd();
        assert_eq!(crd.spec.group, "camelot.pleme.io");
        assert_eq!(crd.spec.names.kind, "RabbitmqTopology");
    }
}
