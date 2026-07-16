//! `camelot-rabbitmq-controller` — M0 (task #150): a real, compiling
//! `RabbitmqTopology` CRD (`camelot.pleme.io/v1`) plus an **observe-only**
//! reconciler that reads the live RabbitMQ management HTTP API and diffs it
//! against the CR's declared vhosts/queues. Modeled on `breathe-crd` +
//! `breathe-controller`'s CRD/reconcile shape (see module docs for the exact
//! parallel and where this M0 deliberately narrows scope).
//!
//! Tier-honest status (never round up): the CRD type and the pure
//! `diff_topology` comparison are real, compiling, and unit-tested with zero
//! network. The `reconcile` fn compiles against `kube`'s real `Client`/
//! `Controller`/`Action` types but has never been run against a live
//! cluster or a live broker in this task — running it, and the CRD's
//! `kubectl apply`, are explicitly out of scope here. Permissions/users and
//! any mutating (create-missing-topology) path are named gaps, not silent
//! omissions — see `crd.rs`'s module doc.

pub mod crd;
pub mod fetch;
pub mod observed;
pub mod reconcile;
