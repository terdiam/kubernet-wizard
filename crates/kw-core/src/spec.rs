//! `ClusterSpec`: the single source of truth persisted as `cluster.yaml`.

use std::net::IpAddr;

use ipnet::IpNet;
use serde::{Deserialize, Serialize};

/// Declares `label()` + `ALL` for a fieldless enum.
macro_rules! labeled_enum {
    ($name:ident { $($var:ident => $label:literal),+ $(,)? }) => {
        impl $name {
            pub const ALL: &'static [Self] = &[$(Self::$var),+];

            #[inline]
            pub const fn label(self) -> &'static str {
                match self { $(Self::$var => $label),+ }
            }
        }
    };
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClusterSpec {
    pub name: String,
    pub target: Target,
    pub distro: DistroSel,
    pub topology: Topology,
    pub endpoint: CpEndpoint,
    pub service_lb: ServiceLb,
    pub network: Network,
    pub ingress: Ingress,
    #[serde(default)]
    pub addons: Vec<Addon>,
    #[serde(default)]
    pub gitops: GitOps,
    #[serde(default)]
    pub tuning: Tuning,
}

impl ClusterSpec {
    pub fn from_yaml(text: &str) -> Result<Self, serde_yaml::Error> {
        serde_yaml::from_str(text)
    }

    pub fn to_yaml(&self) -> Result<String, serde_yaml::Error> {
        serde_yaml::to_string(self)
    }

    #[inline]
    pub fn has_addon(&self, addon: Addon) -> bool {
        self.addons.contains(&addon)
    }

    #[inline]
    pub fn is_cloud(&self) -> bool {
        matches!(self.target, Target::Cloud { .. })
    }

    /// Nodes filtered by role, borrowed.
    pub fn nodes_with_role(&self, role: NodeRole) -> impl Iterator<Item = &Node> {
        self.topology.nodes.iter().filter(move |n| n.role == role)
    }
}

// ---------------------------------------------------------------- target

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Target {
    Baremetal,
    Cloud {
        provider: CloudProvider,
        provisioning: Provisioning,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloudProvider {
    Aws,
    Gcp,
    Azure,
    Linode,
    DigitalOcean,
    Hetzner,
    Proxmox,
}

labeled_enum!(CloudProvider {
    Aws => "AWS",
    Gcp => "Google Cloud",
    Azure => "Azure",
    Linode => "Akamai / Linode",
    DigitalOcean => "DigitalOcean",
    Hetzner => "Hetzner Cloud",
    Proxmox => "Proxmox VE",
});

impl CloudProvider {
    /// Directory name used under `tofu/`.
    #[inline]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Aws => "aws",
            Self::Gcp => "gcp",
            Self::Azure => "azure",
            Self::Linode => "linode",
            Self::DigitalOcean => "digitalocean",
            Self::Hetzner => "hetzner",
            Self::Proxmox => "proxmox",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum Provisioning {
    /// Create VMs with `OpenTofu`, then bootstrap.
    Tofu {
        region: String,
        cp_size: String,
        worker_size: String,
        image: String,
    },
    /// User supplies existing VM addresses in `topology.nodes`.
    ExistingVms,
}

// ---------------------------------------------------------------- distro

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DistroId {
    Kubeadm,
    K3s,
    Rke2,
    K0s,
    Microk8s,
    Charmed,
    Kubesphere,
    Deckhouse,
    Kfd,
    Typhoon,
    Metalk8s,
    Kubecube,
}

labeled_enum!(DistroId {
    Kubeadm => "kubeadm (vanilla)",
    K3s => "k3s",
    Rke2 => "RKE2",
    K0s => "k0s",
    Microk8s => "MicroK8s",
    Charmed => "Charmed Kubernetes",
    Kubesphere => "KubeSphere (KubeKey)",
    Deckhouse => "Deckhouse",
    Kfd => "Kubernetes Fury Distribution",
    Typhoon => "Typhoon",
    Metalk8s => "MetalK8s",
    Kubecube => "KubeCube",
});

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DistroSel {
    pub id: DistroId,
    pub version: String,
}

// ---------------------------------------------------------------- topology

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Topology {
    /// Desired HA. Single CP clusters are still built HA-ready.
    pub ha: bool,
    pub control_planes: u8,
    pub workers: u8,
    pub os: NodeOs,
    #[serde(default)]
    pub nodes: Vec<Node>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Node {
    pub name: String,
    pub address: IpAddr,
    pub role: NodeRole,
    #[serde(default = "default_ssh_user")]
    pub ssh_user: String,
    #[serde(default)]
    pub ssh_key: Option<String>,
}

fn default_ssh_user() -> String {
    "root".to_owned()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeRole {
    ControlPlane,
    Worker,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeOs {
    Ubuntu2204,
    Ubuntu2404,
    Debian12,
    Debian13,
    Rhel9,
}

labeled_enum!(NodeOs {
    Ubuntu2204 => "Ubuntu 22.04 LTS",
    Ubuntu2404 => "Ubuntu 24.04 LTS",
    Debian12 => "Debian 12",
    Debian13 => "Debian 13",
    Rhel9 => "RHEL / Rocky / Alma 9",
});

impl NodeOs {
    #[inline]
    pub const fn family(self) -> &'static str {
        match self {
            Self::Ubuntu2204 | Self::Ubuntu2404 | Self::Debian12 | Self::Debian13 => "debian",
            Self::Rhel9 => "redhat",
        }
    }
}

// ---------------------------------------------------------------- endpoint / LB

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CpEndpoint {
    pub mode: EndpointMode,
    /// Virtual IP (kube-vip / keepalived).
    #[serde(default)]
    pub vip: Option<IpAddr>,
    /// NIC that holds the VIP (ARP mode).
    #[serde(default)]
    pub interface: Option<String>,
    /// Optional DNS name, added to certificate SANs.
    #[serde(default)]
    pub dns_name: Option<String>,
    #[serde(default = "default_api_port")]
    pub port: u16,
}

const fn default_api_port() -> u16 {
    6443
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EndpointMode {
    KubeVip,
    KeepalivedHaproxy,
    CloudLb,
}

labeled_enum!(EndpointMode {
    KubeVip => "kube-vip (VIP)",
    KeepalivedHaproxy => "keepalived + HAProxy",
    CloudLb => "Cloud load balancer",
});

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceLb {
    pub kind: ServiceLbKind,
    #[serde(default)]
    pub advertisement: Advertisement,
    /// Address pool, `a.b.c.d-a.b.c.e` or CIDR.
    #[serde(default)]
    pub pool: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceLbKind {
    Metallb,
    KubeVip,
    CloudCcm,
    None,
}

labeled_enum!(ServiceLbKind {
    Metallb => "MetalLB",
    KubeVip => "kube-vip cloud provider",
    CloudCcm => "Cloud controller (CCM)",
    None => "None",
});

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Advertisement {
    #[default]
    L2,
    Bgp,
}

labeled_enum!(Advertisement { L2 => "Layer 2 (ARP)", Bgp => "BGP" });

// ---------------------------------------------------------------- network

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Network {
    pub cni: Cni,
    pub pod_cidr: IpNet,
    pub service_cidr: IpNet,
    /// Subnet nodes live in; used for overlap / VIP checks.
    #[serde(default)]
    pub node_cidr: Option<IpNet>,
    #[serde(default = "default_domain")]
    pub cluster_domain: String,
    #[serde(default)]
    pub kube_proxy: KubeProxyMode,
    #[serde(default)]
    pub mtu: Option<u16>,
    #[serde(default)]
    pub dual_stack: bool,
}

fn default_domain() -> String {
    "cluster.local".to_owned()
}

impl Network {
    /// Cluster DNS = 10th address of the service CIDR (k8s convention).
    pub fn cluster_dns(&self) -> Option<IpAddr> {
        self.service_cidr.hosts().nth(9)
    }

    /// API service `ClusterIP` = first host of service CIDR.
    pub fn kubernetes_service_ip(&self) -> Option<IpAddr> {
        self.service_cidr.hosts().next()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cni {
    Cilium,
    Calico,
    Flannel,
    Canal,
}

labeled_enum!(Cni {
    Cilium => "Cilium (eBPF)",
    Calico => "Calico",
    Flannel => "Flannel",
    Canal => "Canal",
});

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KubeProxyMode {
    #[default]
    Iptables,
    Ipvs,
    Nftables,
    /// Replaced by CNI (Cilium eBPF).
    None,
}

labeled_enum!(KubeProxyMode {
    Iptables => "iptables",
    Ipvs => "IPVS",
    Nftables => "nftables",
    None => "None (CNI replacement)",
});

// ---------------------------------------------------------------- ingress / addons

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ingress {
    Traefik,
    HaproxyIngress,
    EnvoyGateway,
    CiliumGateway,
    NginxGatewayFabric,
    Contour,
    None,
}

labeled_enum!(Ingress {
    Traefik => "Traefik",
    HaproxyIngress => "HAProxy Ingress",
    EnvoyGateway => "Envoy Gateway (Gateway API)",
    CiliumGateway => "Cilium Gateway API",
    NginxGatewayFabric => "NGINX Gateway Fabric",
    Contour => "Contour",
    None => "None",
});

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Addon {
    CertManager,
    MetricsServer,
    LocalPath,
    Longhorn,
    Openebs,
    RookCeph,
    VictoriaMetrics,
    KubePrometheusStack,
    Loki,
    Velero,
    Kyverno,
    ExternalSecrets,
    SealedSecrets,
    NodeLocalDns,
    Descheduler,
    ClusterAutoscaler,
    Vcluster,
}

labeled_enum!(Addon {
    CertManager => "cert-manager",
    MetricsServer => "metrics-server",
    LocalPath => "local-path-provisioner",
    Longhorn => "Longhorn",
    Openebs => "OpenEBS",
    RookCeph => "Rook-Ceph",
    VictoriaMetrics => "VictoriaMetrics (light monitoring)",
    KubePrometheusStack => "kube-prometheus-stack",
    Loki => "Loki + Vector",
    Velero => "Velero (backup)",
    Kyverno => "Kyverno (policy)",
    ExternalSecrets => "External Secrets",
    SealedSecrets => "Sealed Secrets",
    NodeLocalDns => "NodeLocal DNSCache",
    Descheduler => "Descheduler",
    ClusterAutoscaler => "Cluster Autoscaler",
    Vcluster => "vcluster",
});

impl Addon {
    /// Stable id used for templates / gitops paths.
    pub const fn id(self) -> &'static str {
        match self {
            Self::CertManager => "cert-manager",
            Self::MetricsServer => "metrics-server",
            Self::LocalPath => "local-path",
            Self::Longhorn => "longhorn",
            Self::Openebs => "openebs",
            Self::RookCeph => "rook-ceph",
            Self::VictoriaMetrics => "victoria-metrics",
            Self::KubePrometheusStack => "kube-prometheus-stack",
            Self::Loki => "loki",
            Self::Velero => "velero",
            Self::Kyverno => "kyverno",
            Self::ExternalSecrets => "external-secrets",
            Self::SealedSecrets => "sealed-secrets",
            Self::NodeLocalDns => "node-local-dns",
            Self::Descheduler => "descheduler",
            Self::ClusterAutoscaler => "cluster-autoscaler",
            Self::Vcluster => "vcluster",
        }
    }
}

// ---------------------------------------------------------------- gitops / tuning

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitOps {
    pub tool: GitOpsTool,
    #[serde(default)]
    pub repo_url: String,
    #[serde(default = "default_branch")]
    pub branch: String,
    #[serde(default)]
    pub path: String,
}

fn default_branch() -> String {
    "main".to_owned()
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GitOpsTool {
    #[default]
    None,
    ArgoCd,
    Flux,
}

labeled_enum!(GitOpsTool { None => "None", ArgoCd => "Argo CD", Flux => "Flux" });

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tuning {
    pub profile: TuningProfile,
    /// Expected RAM per node in GiB, drives kubelet reservations.
    #[serde(default)]
    pub node_memory_gib: Option<u16>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TuningProfile {
    #[default]
    Minimal,
    Balanced,
    Throughput,
}

labeled_enum!(TuningProfile {
    Minimal => "Minimal footprint",
    Balanced => "Balanced",
    Throughput => "Throughput",
});
