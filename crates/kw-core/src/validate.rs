//! Spec validation. Produces issues keyed by `Field` so the UI can show them
//! inline and derive per-step status.

use std::borrow::Cow;
use std::net::IpAddr;

use ipnet::IpNet;

use crate::caps::Capabilities;
use crate::compat;
use crate::spec::{
    CloudProvider, ClusterSpec, EndpointMode, GitOpsTool, NodeRole, Provisioning, ServiceLbKind, Target,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Field {
    Name,
    Target,
    Distro,
    Version,
    ControlPlanes,
    Workers,
    Os,
    Nodes,
    EndpointMode,
    Vip,
    Interface,
    ServiceLb,
    LbPool,
    Cni,
    PodCidr,
    ServiceCidr,
    NodeCidr,
    KubeProxy,
    Mtu,
    Ingress,
    GitOpsRepo,
    Provisioning,
}

/// Wizard steps, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Step {
    Project,
    Target,
    Distro,
    Topology,
    Endpoint,
    Network,
    Ingress,
    Features,
    GitOps,
    Tuning,
    Review,
    Deploy,
}

impl Step {
    pub const ALL: [Self; 12] = [
        Self::Project,
        Self::Target,
        Self::Distro,
        Self::Topology,
        Self::Endpoint,
        Self::Network,
        Self::Ingress,
        Self::Features,
        Self::GitOps,
        Self::Tuning,
        Self::Review,
        Self::Deploy,
    ];

    #[inline]
    pub const fn index(self) -> usize {
        self as usize
    }
}

impl Field {
    #[inline]
    pub const fn step(self) -> Step {
        match self {
            Self::Name => Step::Project,
            Self::Target | Self::Provisioning => Step::Target,
            Self::Distro | Self::Version => Step::Distro,
            Self::ControlPlanes | Self::Workers | Self::Os | Self::Nodes => Step::Topology,
            Self::EndpointMode | Self::Vip | Self::Interface | Self::ServiceLb | Self::LbPool => {
                Step::Endpoint
            }
            Self::Cni
            | Self::PodCidr
            | Self::ServiceCidr
            | Self::NodeCidr
            | Self::KubeProxy
            | Self::Mtu => Step::Network,
            Self::Ingress => Step::Ingress,
            Self::GitOpsRepo => Step::GitOps,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    pub field: Field,
    pub severity: Severity,
    pub message: Cow<'static, str>,
}

impl Issue {
    #[inline]
    fn error(field: Field, message: impl Into<Cow<'static, str>>) -> Self {
        Self { field, severity: Severity::Error, message: message.into() }
    }

    #[inline]
    fn warn(field: Field, message: impl Into<Cow<'static, str>>) -> Self {
        Self { field, severity: Severity::Warning, message: message.into() }
    }
}

/// Validation result with fast lookups for the UI.
#[derive(Debug, Default, Clone)]
pub struct Report {
    pub issues: Vec<Issue>,
}

impl Report {
    #[inline]
    pub fn has_errors(&self) -> bool {
        self.issues.iter().any(|i| i.severity == Severity::Error)
    }

    pub fn for_field(&self, field: Field) -> impl Iterator<Item = &Issue> {
        self.issues.iter().filter(move |i| i.field == field)
    }

    /// Worst severity for a step, `None` = clean.
    pub fn step_severity(&self, step: Step) -> Option<Severity> {
        self.issues.iter().filter(|i| i.field.step() == step).map(|i| i.severity).max()
    }
}

pub fn validate(spec: &ClusterSpec, caps: &Capabilities) -> Report {
    let mut issues = Vec::with_capacity(8);

    check_name(spec, &mut issues);
    check_target(spec, &mut issues);
    check_compat(spec, caps, &mut issues);
    check_topology(spec, &mut issues);
    check_endpoint(spec, &mut issues);
    check_network(spec, &mut issues);
    check_gitops(spec, &mut issues);

    Report { issues }
}

fn check_name(spec: &ClusterSpec, out: &mut Vec<Issue>) {
    let n = spec.name.as_str();
    let valid = !n.is_empty()
        && n.len() <= 40
        && n.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !n.starts_with('-')
        && !n.ends_with('-');
    if !valid {
        out.push(Issue::error(
            Field::Name,
            "Use 1-40 chars: lowercase letters, digits and '-' (e.g. prod-cluster)",
        ));
    }
}

fn check_target(spec: &ClusterSpec, out: &mut Vec<Issue>) {
    let Target::Cloud { provider, provisioning } = &spec.target else { return };
    match provisioning {
        Provisioning::Tofu { region, cp_size, worker_size, image } => {
            if [region, cp_size, worker_size, image].iter().any(|v| v.trim().is_empty()) {
                out.push(Issue::error(Field::Provisioning, "Region, sizes and image are required for OpenTofu provisioning"));
            }
            if *provider == CloudProvider::Proxmox {
                if ![cp_size, worker_size].iter().all(|s| is_cores_x_mib(s)) {
                    out.push(Issue::error(Field::Provisioning, "Proxmox size must be <cores>x<MiB>, e.g. 2x4096"));
                }
                if image.trim().parse::<u32>().is_err() {
                    out.push(Issue::error(Field::Provisioning, "Proxmox image must be the VM id of a cloud-init template, e.g. 9000"));
                }
            }
        }
        Provisioning::ExistingVms => {
            if spec.endpoint.mode == EndpointMode::CloudLb && spec.endpoint.dns_name.as_deref().is_none_or(str::is_empty) {
                out.push(Issue::error(
                    Field::EndpointMode,
                    "Enter the DNS name or IP of your load balancer; it is only created automatically with OpenTofu",
                ));
            }
        }
    }
}

/// `2x4096` style Proxmox size.
fn is_cores_x_mib(s: &str) -> bool {
    s.trim().split_once('x').is_some_and(|(c, m)| c.parse::<u16>().is_ok_and(|c| c > 0) && m.parse::<u32>().is_ok_and(|m| m >= 512))
}

fn check_compat(spec: &ClusterSpec, caps: &Capabilities, out: &mut Vec<Issue>) {
    out.extend(
        compat::spec_reasons(spec, caps)
            .into_iter()
            .filter_map(|(f, r)| r.map(|msg| Issue::error(f, msg))),
    );
    if caps.is_ready() && !caps.versions.contains(&spec.distro.version.as_str()) {
        out.push(Issue::warn(Field::Version, "Version not in the tested catalog"));
    }
}

/// Nodes are typed by user unless `OpenTofu` provisions them.
#[inline]
fn needs_inventory(spec: &ClusterSpec) -> bool {
    !matches!(spec.target, Target::Cloud { provisioning: Provisioning::Tofu { .. }, .. })
}

fn check_topology(spec: &ClusterSpec, out: &mut Vec<Issue>) {
    let t = &spec.topology;
    match (t.ha, t.control_planes) {
        (true, 3 | 5) | (false, 1) => {}
        (true, _) => out.push(Issue::error(
            Field::ControlPlanes,
            "HA needs 3 or 5 control planes (odd count keeps etcd quorum)",
        )),
        (false, _) => out.push(Issue::error(
            Field::ControlPlanes,
            "Non-HA uses exactly 1 control plane (enable HA for more)",
        )),
    }
    if t.workers == 0 {
        out.push(Issue::warn(
            Field::Workers,
            "No workers: workloads will run on control planes (taint removed)",
        ));
    }

    if !needs_inventory(spec) {
        return;
    }
    let cps = spec.nodes_with_role(NodeRole::ControlPlane).count();
    let workers = spec.nodes_with_role(NodeRole::Worker).count();
    if cps != usize::from(t.control_planes) || workers != usize::from(t.workers) {
        out.push(Issue::error(
            Field::Nodes,
            format!(
                "Inventory has {cps} control plane(s) and {workers} worker(s); expected {} and {}",
                t.control_planes, t.workers
            ),
        ));
    }
    // Duplicate addresses.
    let mut addrs: Vec<IpAddr> = t.nodes.iter().map(|n| n.address).collect();
    addrs.sort_unstable();
    if addrs.windows(2).any(|w| w[0] == w[1]) {
        out.push(Issue::error(Field::Nodes, "Duplicate node address in inventory"));
    }
    if let Some(cidr) = spec.network.node_cidr
        && let Some(n) = t.nodes.iter().find(|n| !cidr.contains(&n.address))
    {
        out.push(Issue::warn(
            Field::Nodes,
            format!("Node {} ({}) is outside node subnet {cidr}", n.name, n.address),
        ));
    }
}

fn check_endpoint(spec: &ClusterSpec, out: &mut Vec<Issue>) {
    let ep = &spec.endpoint;
    match ep.mode {
        EndpointMode::KubeVip | EndpointMode::KeepalivedHaproxy => match ep.vip {
            None => out.push(Issue::error(
                Field::Vip,
                "A virtual IP is required, even for 1 control plane, so the cluster can be upgraded to HA later",
            )),
            Some(vip) => {
                if spec.topology.nodes.iter().any(|n| n.address == vip) {
                    out.push(Issue::error(Field::Vip, "VIP must not equal a node address"));
                }
                if let Some(cidr) = spec.network.node_cidr
                    && !cidr.contains(&vip)
                {
                    out.push(Issue::error(
                        Field::Vip,
                        format!("VIP {vip} is outside node subnet {cidr}; pick a free IP inside it"),
                    ));
                }
            }
        },
        EndpointMode::CloudLb => {}
        EndpointMode::Dns => {
            if ep.dns_name.as_deref().is_none_or(|d| d.trim().is_empty()) {
                out.push(Issue::error(
                    Field::EndpointMode,
                    "Enter the API DNS name (e.g. api.example.com); it stays the endpoint when you add control planes",
                ));
            }
        }
    }
    if ep.mode.uses_vip() && ep.interface.as_deref().is_none_or(str::is_empty) {
        out.push(Issue::warn(Field::Interface, "Interface not set; first default-route NIC is used"));
    }

    let lb = &spec.service_lb;
    if matches!(lb.kind, ServiceLbKind::Metallb | ServiceLbKind::KubeVip) {
        match lb.pool.as_deref().map(parse_pool) {
            None => out.push(Issue::error(Field::LbPool, "Address pool is required (e.g. 192.168.1.240-192.168.1.250)")),
            Some(None) => out.push(Issue::error(Field::LbPool, "Pool must be 'start-end' IP range or CIDR")),
            Some(Some((start, end))) => {
                if ep.vip.is_some_and(|v| v >= start && v <= end) {
                    out.push(Issue::error(Field::LbPool, "Pool overlaps the control-plane VIP"));
                }
            }
        }
    }
}

/// Parses `a-b` range or CIDR into inclusive bounds.
pub fn parse_pool(text: &str) -> Option<(IpAddr, IpAddr)> {
    let text = text.trim();
    if let Some((a, b)) = text.split_once('-') {
        let (a, b): (IpAddr, IpAddr) = (a.trim().parse().ok()?, b.trim().parse().ok()?);
        return (a.is_ipv4() == b.is_ipv4() && a <= b).then_some((a, b));
    }
    let net: IpNet = text.parse().ok()?;
    Some((net.network(), net.broadcast()))
}

#[inline]
fn overlaps(a: IpNet, b: IpNet) -> bool {
    a.contains(&b.network()) || b.contains(&a.network())
}

fn check_network(spec: &ClusterSpec, out: &mut Vec<Issue>) {
    let net = &spec.network;
    if overlaps(net.pod_cidr, net.service_cidr) {
        out.push(Issue::error(
            Field::ServiceCidr,
            format!("Service CIDR {} overlaps pod CIDR {}", net.service_cidr, net.pod_cidr),
        ));
    }
    if let Some(node) = net.node_cidr {
        for (field, cidr, label, hint) in [
            (Field::PodCidr, net.pod_cidr, "Pod", "10.42.0.0/16"),
            (Field::ServiceCidr, net.service_cidr, "Service", "10.43.0.0/16"),
        ] {
            if overlaps(node, cidr) {
                out.push(Issue::error(
                    field,
                    format!("{label} CIDR {cidr} overlaps node subnet {node}; try {hint}"),
                ));
            }
        }
    }
    if net.pod_cidr.prefix_len() > 20 && net.pod_cidr.addr().is_ipv4() {
        out.push(Issue::warn(Field::PodCidr, "Pod CIDR smaller than /20 limits nodes (each node takes a /24)"));
    }
    if net.service_cidr.prefix_len() < 12 && net.service_cidr.addr().is_ipv4() {
        out.push(Issue::error(Field::ServiceCidr, "Service CIDR larger than /12 is rejected by kube-apiserver"));
    }
    if let Some(mtu) = net.mtu
        && !(1280..=9000).contains(&mtu)
    {
        out.push(Issue::error(Field::Mtu, "MTU must be between 1280 and 9000"));
    }
}

fn check_gitops(spec: &ClusterSpec, out: &mut Vec<Issue>) {
    let g = &spec.gitops;
    if g.tool == GitOpsTool::None {
        return;
    }
    let url = g.repo_url.trim();
    let ok = url.starts_with("https://") || url.starts_with("ssh://") || url.starts_with("git@");
    if !ok {
        out.push(Issue::error(Field::GitOpsRepo, "Repository URL must start with https://, ssh:// or git@"));
    }
}
