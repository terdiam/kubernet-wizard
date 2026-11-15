# Prompt for Claude Design: kubernet-wizard

Copy everything below the line into Claude Design.

---

Design a desktop application UI for **kubernet-wizard**: a cross-platform desktop wizard (macOS, Windows, Linux) that helps people design a Kubernetes cluster, then generates an OpenTofu + Ansible + GitOps project and deploys it with live logs. Audience: platform engineers and homelab users. Some are experts; many are new to Kubernetes. The UI must feel calm, guided and trustworthy, never overwhelming.

## Platform constraints
- Native desktop app built with GPUI (Rust) and the gpui-kit component library (shadcn-like: Button, Input, Select, Switch, Checkbox, chips/segmented choices, Tabs, Dialog, Tooltip, Notification, Popover, scrollable panels, Markdown viewer). Design only with components like these; no web-only effects such as heavy blur or video.
- Default window 1280×820. Minimum 1024×700. Layout must work at both sizes.
- Icons: Lucide only.
- Light and dark theme, following the OS. Show both themes.
- Bilingual UI: Bahasa Indonesia and English. Keep labels short, and leave room for Indonesian, which runs ~20% longer.

## Brand
- Logo: a white hexagon outline (the cluster) holding three connected nodes, with two gold sparkles (the wizard), on an indigo→teal diagonal gradient rounded square.
- Colors: indigo `#3B5BDB` (primary), teal `#12B8A6` (accent and success), gold `#FFD43B` (highlight, use sparingly).
- Neutral, technical, friendly. Monospace font for code, logs, IPs and CIDRs.

## UX rules (must follow)
1. Flow: Welcome → prerequisite check → optional guided tour → Home → Wizard (12 steps) → Review → Deploy → Day-2 cluster screen.
2. Progressive disclosure: global **Basic / Advanced** toggle. Basic shows only essential fields with safe defaults; Advanced reveals CIDRs, MTU, kube-proxy mode, tuning overrides.
3. Recommended options carry a **"Recommended"** badge and a one-line reason.
4. Incompatible options are **disabled with a tooltip explaining why**, never hidden. Example: "kube-proxy none requires Cilium"; "Cloud networks cannot float a virtual IP — use the cloud load balancer or a DNS name".
5. Stepper is strictly sequential. Users can go back to any earlier step, but cannot jump ahead until the current step is complete. Future steps show a lock icon with the tooltip "Finish the current step first". Per-step status uses icon + text, never color alone: ✓ valid, ⚠ warning, ✕ error, 🔒 locked. Only errors block Next; warnings allow continuing.
6. Inline validation sits under each field: the problem plus a concrete fix. Example: "Pod CIDR overlaps node subnet 10.0.0.0/24 — try 10.42.0.0/16".
7. Every field has a `?` help icon that opens docs at the matching anchor.
8. A right-hand context panel explains whichever option is focused or hovered: what it is, pros and cons, when to use it, and a "Learn more" link.
9. Destructive actions (tofu apply/destroy, overwriting a folder) open a confirm dialog with an impact summary.
10. Statuses always pair an icon with text.

## Screens to design

### 1. Welcome (first run)
Logo (88px), a one-sentence value proposition, and a prerequisite checklist: OpenTofu, Ansible ≥ 2.16 (via WSL2 on Windows), kubectl, Helm, Git, SSH. Each item shows its version and a status (found / outdated / missing) with a "How to install" link. Buttons: "Start tour" and "Skip".

### 2. Home
Logo (64px). Four large actions: New cluster, Open project, Start from preset, Docs. Preset cards, each with an icon, a short spec summary and its footprint:
- Dev local (k3s single, Flannel, Traefik) — laptop icon
- Homelab baremetal (k3s HA, kube-vip, MetalLB, Cilium) — house icon
- Production on-prem (kubeadm/RKE2 3 CP, Cilium, Envoy Gateway, Argo CD) — building icon
- Cloud via OpenTofu (Hetzner/AWS…) — cloud icon

Below: a recent projects list.

### 3. Wizard shell (the core screen)
- **Header:** app logo + name; Home / Wizard nav; Basic/Advanced toggle; undo/redo; ID/EN language; "Update available" pill; settings.
- **Left stepper (200px):** 12 steps, each with a Lucide icon and a status icon:
  1. Project (folder-open)
  2. Target (cloud)
  3. Distro (boxes)
  4. Topology (server)
  5. Endpoint & LB (waypoints)
  6. Network (network)
  7. Ingress (route)
  8. Features (puzzle)
  9. GitOps (git-branch)
  10. Tuning (gauge)
  11. Review (clipboard-check)
  12. Deploy (rocket)

  Show these states: current, completed, warning, error, and locked future steps.
- **Center:** step title with icon, a short description, then form fields. Use chip groups for single-choice options.
- **Right context panel (280px, collapsible):**
  - Help for the focused option.
  - Live summary card: distro, nodes, HA, CNI, endpoint, service LB, ingress, GitOps.
  - Resource estimate: system RAM/CPU per node, with a warning when nodes are too small.
  - For cloud targets: a rough monthly cost, labelled "estimate".
- **Footer:** Back on the left. Primary "Next · <next step name>" on the right; when blocked it is disabled, with "Fix errors before continuing".

### 4. Step designs (center content)
- **Project:** name, output folder picker, open an existing `cluster.yaml`.
- **Target:** Baremetal/existing servers vs Cloud. For Cloud, show:
  - Provider chips: AWS, Google Cloud, Azure, Akamai/Linode, DigitalOcean, Hetzner, Proxmox.
  - "Create with OpenTofu" vs "Use existing VMs".
  - Region, control-plane size, worker size and image fields, prefilled per provider.
  - A credential hint line, e.g. "Reads HCLOUD_TOKEN from environment".
- **Distro:** cards for kubeadm, k3s and RKE2 (ready), each with a description and a version select. Planned distros (k0s, MicroK8s, Charmed, KubeSphere, Deckhouse, KFD, Typhoon, MetalK8s, KubeCube) appear as disabled cards with a "Coming soon" badge.
- **Topology:**
  - HA switch: 1 control plane, or 3/5.
  - Workers stepper.
  - Node OS chips: Ubuntu 22.04, 24.04 (Recommended), 26.04, Debian 12/13, RHEL/Rocky/Alma 9.
  - Node inventory table: name, IP, SSH user, SSH key, role.
  - A **live topology diagram**: control-plane boxes, worker boxes, VIP/LB/DNS endpoint box and traffic arrows, updating as counts change.
- **Endpoint & LB:**
  - Control-plane endpoint chips: kube-vip (VIP), keepalived + HAProxy, Cloud load balancer, **DNS name (no VIP)**.
  - VIP + interface fields, shown only for VIP modes.
  - DNS name field, required for DNS mode.
  - An info callout: "A stable endpoint is required even for 1 control plane so you can upgrade to HA later."
  - Show the cloud case: VIP modes disabled with a tooltip, and DNS/Cloud LB enabled.
  - Service LoadBalancer chips: MetalLB, kube-vip cloud provider, Cloud CCM, None. Then L2/BGP and an address pool.
- **Network:** CNI cards (Cilium, Calico, Flannel, Canal) with memory footprint. In Advanced mode: pod CIDR, service CIDR, derived cluster DNS IP (read-only), domain, kube-proxy mode, MTU, dual-stack.
- **Ingress:** Traefik, Envoy Gateway, Cilium Gateway, NGINX Gateway Fabric, HAProxy, Contour, None. Include a default IngressClass toggle.
- **Features:** add-on toggles grouped as:
  - Storage: local-path, Longhorn, OpenEBS, Rook-Ceph, cloud CSI.
  - Observability: metrics-server, VictoriaMetrics, Loki/Vector.
  - Security: cert-manager, Kyverno, external-secrets/sealed-secrets.
  - Ops: Velero, descheduler, node-local-dns, cluster-autoscaler, vcluster.
- **GitOps:**
  - Tool chips: None, Argo CD, Flux.
  - Repo provider, URL, branch and path.
  - A generated ed25519 deploy public key with a copy button.
- **Tuning:** profile cards (Minimal footprint — Recommended; Balanced; Throughput), each listing what it changes. Advanced overrides go in a collapsible section.

### 5. Review
- Validation issues list: icon, message, and a "Go to step" button for each.
- Split view: a generated file tree (`cluster.yaml`, `tofu/…`, `ansible/…`, `gitops/…`) and a monospace file preview.
- Primary action: "Generate project".

### 6. Deploy
- Phase timeline: Validate → Provision (tofu) → Bootstrap → CNI → Add-ons → GitOps → Smoke test. Each phase shows status and duration.
- Live log pane (monospace, auto-scroll) with an "errors only" filter, copy and cancel.
- A `tofu plan` summary inside a confirm dialog before apply.
- Failure state: a mapped explanation, fix steps, and "Retry from this phase".
- Success state: nodes Ready, kubeconfig path with a copy button, "Open cluster".

### 7. Day-2 cluster screen
- Cluster card: health, nodes and endpoint.
- Actions: "Upgrade to HA" (adds 2 control planes), "Add workers", "Re-run tuning". Each opens a confirm dialog with an impact summary.

### 8. Docs viewer
- Left table of contents.
- Search box with results.
- Markdown content with anchors and code blocks.

### 9. Guided tour overlay
- Dimmed backdrop with a spotlight on the target element.
- Tooltip card: title, body, progress "3/9", and Back / Next / Skip.

### 10. Dialogs and small states
- Update dialog: version, changelog, download progress, "Restart & update". For Linux packages, show "Update via your package manager" with an "Open release page" button.
- Destructive confirm dialog.
- Settings: language, theme, font size, update channel stable/beta, auto-check.
- Toast notifications.
- Empty state.
- Loading skeletons.

## Deliverables
- High-fidelity mockups of every screen above, in light and dark.
- Detailed wizard frames for the Topology and Endpoint & LB steps, including the disabled-with-tooltip, inline error and locked-stepper states.
- A small component sheet: chip group (default / selected / recommended / disabled), field with `?` help and inline error, stepper item in all five states, status badge, context panel, log line styles.
- Design tokens (colors, radius, spacing, type scale) that can be mapped to a gpui-kit theme.
