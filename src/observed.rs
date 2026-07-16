//! Typed shapes for what the RabbitMQ management HTTP API actually returns
//! (`GET /api/vhosts`, `GET /api/queues`), and the pure diff against a
//! declared [`crate::crd::RabbitmqTopologySpec`]. Split deliberately from
//! `fetch.rs`'s I/O so `diff_topology` is unit-testable with zero network —
//! the same pure-core/side-effect-seam discipline the TYPED-SPEC triplet
//! asks for, without pulling in a full mockable `Environment` trait for a
//! single read call (that generalization is the natural M1 move once a
//! write path exists to mock too).

use serde::Deserialize;

/// One row of `GET /api/vhosts`.
#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ObservedVhost {
    pub name: String,
}

/// One row of `GET /api/queues` — the fields this M0 cares about; the real
/// API returns many more (messages, consumers, memory, ...) that a future
/// milestone can add without breaking this one (`#[serde(default)]` +
/// unknown fields are simply ignored by serde_json by default).
#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ObservedQueue {
    pub name: String,
    pub vhost: String,
    #[serde(default)]
    pub durable: bool,
}

/// The assembled live-broker snapshot this reconciler compares against spec.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ObservedTopology {
    pub vhosts: Vec<ObservedVhost>,
    pub queues: Vec<ObservedQueue>,
}

/// One unit of drift between declared and observed state. Deliberately a
/// flat enum of *facts*, not a decision — M0 never decides what to do about
/// a drift, only reports it (observe-only, per task scope).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Drift {
    MissingVhost { vhost: String },
    MissingQueue { vhost: String, queue: String },
    QueueDurabilityMismatch { vhost: String, queue: String, desired_durable: bool, observed_durable: bool },
    UndeclaredVhost { vhost: String },
}

impl std::fmt::Display for Drift {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Drift::MissingVhost { vhost } => write!(f, "vhost '{vhost}' declared but absent on the broker"),
            Drift::MissingQueue { vhost, queue } => {
                write!(f, "queue '{queue}' declared in vhost '{vhost}' but absent on the broker")
            }
            Drift::QueueDurabilityMismatch { vhost, queue, desired_durable, observed_durable } => write!(
                f,
                "queue '{vhost}/{queue}' durable mismatch: desired={desired_durable} observed={observed_durable}"
            ),
            Drift::UndeclaredVhost { vhost } => write!(f, "vhost '{vhost}' exists on the broker but is not declared"),
        }
    }
}

/// The pure comparison. Never touches the network; every case is a fact
/// derivable from the two typed inputs alone.
#[must_use]
pub fn diff_topology(desired: &crate::crd::RabbitmqTopologySpec, observed: &ObservedTopology) -> Vec<Drift> {
    let mut drift = Vec::new();

    for want_vhost in &desired.vhosts {
        let vhost_present = observed.vhosts.iter().any(|v| v.name == want_vhost.name);
        if !vhost_present {
            drift.push(Drift::MissingVhost { vhost: want_vhost.name.clone() });
            // No point checking this vhost's queues if the vhost itself is
            // missing — every queue in it is trivially missing too, and that
            // would just be noise on top of the one real root cause.
            continue;
        }

        for want_queue in &want_vhost.queues {
            match observed
                .queues
                .iter()
                .find(|q| q.vhost == want_vhost.name && q.name == want_queue.name)
            {
                None => drift.push(Drift::MissingQueue { vhost: want_vhost.name.clone(), queue: want_queue.name.clone() }),
                Some(found) if found.durable != want_queue.durable => drift.push(Drift::QueueDurabilityMismatch {
                    vhost: want_vhost.name.clone(),
                    queue: want_queue.name.clone(),
                    desired_durable: want_queue.durable,
                    observed_durable: found.durable,
                }),
                Some(_) => {}
            }
        }
    }

    for seen_vhost in &observed.vhosts {
        if !desired.vhosts.iter().any(|v| v.name == seen_vhost.name) {
            drift.push(Drift::UndeclaredVhost { vhost: seen_vhost.name.clone() });
        }
    }

    drift
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crd::{QueueSpec, RabbitmqTopologySpec, VhostSpec};

    fn spec(vhosts: Vec<VhostSpec>) -> RabbitmqTopologySpec {
        RabbitmqTopologySpec {
            management_api: "http://rabbitmq.camelot.svc.cluster.local:15672".to_string(),
            credentials_secret_ref: "rabbitmq-admin-credentials".to_string(),
            vhosts,
        }
    }

    #[test]
    fn matching_state_produces_no_drift() {
        let desired = spec(vec![VhostSpec {
            name: "akeyless".to_string(),
            queues: vec![QueueSpec { name: "gator-events".to_string(), durable: true }],
        }]);
        let observed = ObservedTopology {
            vhosts: vec![ObservedVhost { name: "akeyless".to_string() }],
            queues: vec![ObservedQueue { name: "gator-events".to_string(), vhost: "akeyless".to_string(), durable: true }],
        };
        assert_eq!(diff_topology(&desired, &observed), vec![]);
    }

    #[test]
    fn missing_vhost_is_reported_and_suppresses_its_queue_noise() {
        let desired = spec(vec![VhostSpec {
            name: "transactions-bis".to_string(),
            queues: vec![QueueSpec { name: "bis-events".to_string(), durable: true }],
        }]);
        let observed = ObservedTopology::default();
        assert_eq!(
            diff_topology(&desired, &observed),
            vec![Drift::MissingVhost { vhost: "transactions-bis".to_string() }]
        );
    }

    #[test]
    fn missing_queue_in_present_vhost_is_reported() {
        let desired = spec(vec![VhostSpec {
            name: "gateway".to_string(),
            queues: vec![QueueSpec { name: "gw-audit".to_string(), durable: true }],
        }]);
        let observed = ObservedTopology {
            vhosts: vec![ObservedVhost { name: "gateway".to_string() }],
            queues: vec![],
        };
        assert_eq!(
            diff_topology(&desired, &observed),
            vec![Drift::MissingQueue { vhost: "gateway".to_string(), queue: "gw-audit".to_string() }]
        );
    }

    #[test]
    fn durability_mismatch_is_reported() {
        let desired = spec(vec![VhostSpec {
            name: "akeyless".to_string(),
            queues: vec![QueueSpec { name: "gator-events".to_string(), durable: true }],
        }]);
        let observed = ObservedTopology {
            vhosts: vec![ObservedVhost { name: "akeyless".to_string() }],
            queues: vec![ObservedQueue { name: "gator-events".to_string(), vhost: "akeyless".to_string(), durable: false }],
        };
        assert_eq!(
            diff_topology(&desired, &observed),
            vec![Drift::QueueDurabilityMismatch {
                vhost: "akeyless".to_string(),
                queue: "gator-events".to_string(),
                desired_durable: true,
                observed_durable: false,
            }]
        );
    }

    #[test]
    fn undeclared_vhost_on_broker_is_reported() {
        let desired = spec(vec![]);
        let observed = ObservedTopology {
            vhosts: vec![ObservedVhost { name: "/".to_string() }],
            queues: vec![],
        };
        assert_eq!(diff_topology(&desired, &observed), vec![Drift::UndeclaredVhost { vhost: "/".to_string() }]);
    }

    #[test]
    fn parses_real_shaped_management_api_json() {
        // Shaped after the actual `GET /api/queues` response fields (extra
        // fields like `messages`/`consumers`/`memory` are present on the
        // real API and simply ignored here — this is the "not yet modeled,
        // named honestly" surface, not a silent gap).
        let raw = r#"[
            {"name":"gator-events","vhost":"akeyless","durable":true,"messages":0,"consumers":1},
            {"name":"bis-events","vhost":"transactions-bis","durable":false,"messages":12,"consumers":0}
        ]"#;
        let queues: Vec<ObservedQueue> = serde_json::from_str(raw).expect("parse queues");
        assert_eq!(queues.len(), 2);
        assert_eq!(queues[0].name, "gator-events");
        assert!(queues[0].durable);
        assert!(!queues[1].durable);
    }
}
