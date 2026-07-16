# camelot-rabbitmq-controller

M0 (task #150): a real, compiling `RabbitmqTopology` CRD (`camelot.pleme.io/v1`)
plus an **observe-only** reconciler that reads the live RabbitMQ management
HTTP API (`/api/vhosts`, `/api/queues`) and diffs it against the CR's declared
vhosts/queues, following `breathe-crd`'s CRD shape and `breathe-controller`'s
`reconcile(obj, ctx) -> Action` pattern.

## What's real vs. stub (tier-honest, never rounded up)

| Piece | Status |
|---|---|
| `RabbitmqTopology` CRD (spec: `managementApi`/`credentialsSecretRef`/`vhosts[].queues[]`; status: `phase`/`driftCount`/`drift`) | **Compiles, tested** — `cargo test` covers YAML round-trip, default fill-in, and `RabbitmqTopology::crd()` generation (9 tests, all green). |
| `diff_topology` (pure compare: missing vhost/queue, durability mismatch, undeclared vhost) | **Compiles, tested**, zero network — the pure core the fleet's TYPED-SPEC discipline asks for. |
| `fetch_topology` (calls the real management API's two list endpoints) | **Compiles**, not exercised against a live broker in this task (no network access from this task's sandbox; the shape mirrors the real API's documented fields). |
| `reconcile()` (`kube::runtime::controller::Action`-returning fn, status patched via SSA) | **Compiles** against real `kube` types. **Never run** — no live cluster reconcile in this task's scope. |
| `main.rs` (wires one `Controller` over `RabbitmqTopology`) | **Compiles as a binary.** `SecretCredentialResolver::resolve` is an honest stub — always returns `Error::CredentialsUnresolved` today; wiring a real async Secret read is the concrete M1 step, named rather than faked. |
| Mutating (create-missing-vhost/queue) path | **Does not exist.** Explicitly out of scope for M0 — this milestone is READ + DIFF only. |
| CRD applied to a live cluster | **Not done** — per task scope, review first. |
| `flake.nix` / repo-forge scaffold | **Not done** — a Cargo-only crate for M0; the nix build wrapper is a named follow-up, not silently skipped. |

## Relationship to the existing IaC path — RESOLVED, retired (2026-07-16)

`pangea-architectures/workspaces/camelot-rabbitmq-topology` declares
vhosts/queues via the `rabbitmq` Terraform provider through pangea-operator +
magma (the declare-and-observe IaC path). The open question this README
originally left for M1 — "is a second, complementary mechanism actually
needed?" — is now answered: **no. This crate is retired; do not build M1.**

Evidence (live-verified against the real cluster, read-only):

- pangea-operator's `InfrastructureTemplate` controller (`handle_ready`/
  `handle_drifted` in `pangea-operator/src/controller/template_controller.rs`)
  already runs a continuous, `refreshInterval`-driven plan→diff→drift cycle
  forever, independent of spec changes, and can auto-correct drift with one
  config flag (`spec.autoApprove: true` — the code path already ships). This
  is not a re-apply-on-change mechanism; live evidence on `camelot-flux-
  bootstrap` shows `cycleCount=535` over 4d9h at a 10m `refreshInterval`.
- `camelot-rabbitmq-topology` itself is real, live, plan-approved (14
  creates) — its one open gap is that the deployed pangea-operator image
  predates the `cyrilgdn_rabbitmq` provider binary `flake.nix` already
  declares, so every apply retry fires `AnomalyProviderUnavailable`. That is
  an **operator-image-rebuild gap** (tracked with task #160's Harbor outage,
  since Harbor is the release path), not an architecture gap — finishing
  this crate would not touch it.
- Building out this crate's M1 (credential resolution, a mutating path, a
  live cluster run) would duplicate machinery pangea-operator already ships
  generically for every `InfrastructureTemplate` — a phase FSM, drift
  diffing, typed `status.conditions`, Prometheus gauges, K8s events — with a
  *thinner* observability surface (`driftCount`/`drift` fields only) than
  what already exists. That is the exact pattern ★★ PLATFORM-MEDIATED
  INFRASTRUCTURE forbids: a second reconciler for a resource the operator
  already owns.

**Disposition:** this repo stays as-is — an archived M0 design reference (the
`RabbitmqTopology` CRD type + the pure `diff_topology` core are real, tested,
reusable if a genuinely distinct future need arises, e.g. observing topology
*before* a provider binary is available as a deliberate, time-boxed stopgap —
not what's being asked here). It is not deleted, and it is not developed
further. Task #150 closes as: resolved via (a); the one real open item
(rebuilding the pangea-operator image with the bundled RabbitMQ provider) is
tracked separately, gated on the Harbor credential outage.
