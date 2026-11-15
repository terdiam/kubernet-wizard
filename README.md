# kubernet-wizard

Cross-platform desktop wizard (Rust + GPUI) to design, generate and deploy Kubernetes clusters.
It renders an OpenTofu + Ansible + GitOps project from a single `cluster.yaml` and runs it with live logs.

## Features

- Targets: baremetal / existing VMs, or cloud with OpenTofu provisioning (Hetzner template today; AWS, GCP, Azure, DigitalOcean, Linode, Proxmox planned).
- Distros: kubeadm, k3s, RKE2 (ready); k0s, MicroK8s, Charmed, KubeSphere, Deckhouse, KFD, Typhoon, MetalK8s, KubeCube (planned, plugin registry).
- Single control plane built HA-ready (stable VIP/LB endpoint + etcd), upgradable to 3/5 CPs in place.
- Endpoint: kube-vip, keepalived + HAProxy, or cloud LB. Service LB: MetalLB, kube-vip, cloud CCM.
- CNI: Cilium (optional kube-proxy replacement), Calico, Flannel, Canal. Custom pod/service CIDR, DNS, MTU.
- Ingress / Gateway API: Traefik, Envoy Gateway, Cilium Gateway, NGINX Gateway Fabric, HAProxy, Contour.
- Add-ons (storage, observability, security, ops) via Helm or GitOps (Argo CD / Flux).
- Tuning profiles (minimal / balanced / throughput): sysctl, THP, journald, idle services, kubelet reservations.
- UX: presets, Basic/Advanced mode, disabled options with reasons, inline validation, context panel,
  footprint estimate, topology preview, file review, demo mode, in-app docs with search, guided tour, ID/EN.

## Workspace

| Crate | Role |
|-------|------|
| `kw-core` | `ClusterSpec`, validation, compatibility rules, presets, HA planner, footprint |
| `kw-distro` | Distro plugins (static enum dispatch) |
| `kw-render` | Embedded templates → project files |
| `kw-exec` | Prerequisite checks, OpenTofu/Ansible runner with streamed events |
| `kw-cli` | `kw` headless CLI |
| `kw-app` | GPUI desktop app |

## Usage

```bash
cargo run -p kw-app --release          # desktop app
cargo run -p kw-cli -- presets          # CLI
cargo run -p kw-cli -- generate --spec examples/k3s-ha.yaml --out /tmp/kw-out
cargo run -p kw-cli -- deploy --spec examples/k3s-ha.yaml --out /tmp/kw-out
```

Requirements on the operator machine: Ansible ≥ 2.16 (WSL2 on Windows), kubectl, Helm, SSH; OpenTofu for cloud provisioning.

## Development

```bash
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo sweep --time 7   # prune old build artefacts
```

## License

Apache License 2.0 — see [LICENSE](LICENSE) and [NOTICE](NOTICE).
