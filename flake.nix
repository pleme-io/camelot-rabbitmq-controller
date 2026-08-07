# camelot-rabbitmq-controller — built the ONE pleme-io Rust way.
#
# Build pattern: substrate's gen-driven `lockfile-builder.mkProject` over the
# committed `Cargo.gen.lock` delta (per-crate buildRustCrate derivations).
# NO buildRustPackage, NO fenix, NO crate2nix. After any `Cargo.lock` change run
# `gen build` to re-emit `Cargo.gen.lock` (the gitignored
# `Cargo.build-spec.json` is the derived full spec the builder consumes).
# Modelled on `pleme-io/escuta`, whose header records why the
# `substrate.rust.*` shapes are the wrong wrapper here: every shape dispatches
# to the same `mkRustToolFlake`, and NONE of them exposes an `image` output —
# that lives on a different, unwired builder.
#
# This is a SINGLE-crate repo, so `project.rootCrate.build` is the accessor
# (escuta uses `project.workspaceMembers.<m>.build` because it is a workspace).
#
# Why an image and not a crate release: per the fleet's AUTO-RELEASE doctrine a
# service or app ships as a Docker IMAGE, not a crates.io crate. This is an
# observe-only reconciler — it belongs in a cluster, not in a dependency graph.
#
# Packages: nix build .#camelot-rabbitmq-controller   (the bin)
# Images:   nix build .#image                          (Linux; darwin gets a stub)
{
  description = "camelot-rabbitmq-controller — typed RabbitmqTopology CRD + observe-only drift reconciler over the RabbitMQ management API";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-25.11";
    flake-utils.url = "github:numtide/flake-utils";
    substrate = {
      url = "github:pleme-io/substrate";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, substrate }:
    flake-utils.lib.eachSystem [
      "x86_64-linux"
      "aarch64-linux"
      "x86_64-darwin"
      "aarch64-darwin"
    ] (system: let
      pkgs = import nixpkgs { inherit system; };

      plemeCrateOverrides = import "${substrate}/lib/build/rust/pleme-crate-overrides.nix";
      lockfileBuilder = import "${substrate}/lib/build/rust/lockfile-builder.nix" { inherit pkgs; };
      project = lockfileBuilder.mkProject {
        src = self;
        defaultCrateOverrides = pkgs.defaultCrateOverrides // plemeCrateOverrides;
      };

      controller = project.rootCrate.build;

      # Lazy on purpose: `dockerTools.buildLayeredImage` asserts on the host
      # platform, so this must never be constructed against darwin `pkgs`.
      hardened =
        if pkgs.stdenv.isLinux
        then import "${substrate}/lib/build/oci/hardened-base.nix" { inherit pkgs system; }
        else null;

      # distroless-glibc: no shell, no package manager, TLS roots + a nonroot
      # (65532:65532) passwd/group stub. `mkPackageImage` is the SAME primitive
      # every other pleme-io Rust image uses — not a bespoke
      # `dockerTools.buildLayeredImage` call.
      image =
        if pkgs.stdenv.isLinux then
          hardened.mkPackageImage {
            service = "camelot-rabbitmq-controller";
            base = hardened.bases.distroless-glibc;
            package = controller;
            publishName = "ghcr.io/pleme-io/camelot-rabbitmq-controller";
            publishTag = "0.1.0";
            entrypoint = [ "${controller}/bin/camelot-rabbitmq-controller" ];
            env = [
              "SSL_CERT_FILE=${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt"
              "RUST_LOG=info,camelot_rabbitmq_controller=info"
            ];
            labels = {
              "org.opencontainers.image.description" =
                "Observe-only RabbitmqTopology reconciler: reads live vhost/queue state from the RabbitMQ management API and diffs it against declared spec. Never mutates the broker.";
              "org.opencontainers.image.licenses" = "MIT";
            };
          }
        else
          pkgs.runCommand "camelot-rabbitmq-controller-image-darwin-stub" {} ''
            mkdir -p $out
            echo "Build the OCI image on Linux: nix build .#image --system x86_64-linux" > $out/README
          '';

    in {
      packages = {
        default = controller;
        camelot-rabbitmq-controller = controller;
        inherit image;
      };
      apps.default = {
        type = "app";
        program = "${controller}/bin/camelot-rabbitmq-controller";
      };
      devShells.default = pkgs.mkShellNoCC {
        buildInputs = with pkgs; [ cargo rustc pkg-config kubectl ];
      };
    });
}
