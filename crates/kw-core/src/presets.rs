//! Quick-start presets shown on the Home screen.

use std::net::{IpAddr, Ipv4Addr};

use ipnet::{IpNet, Ipv4Net};

use crate::spec::{
    Addon, Advertisement, ClusterSpec, CloudProvider, Cni, CpEndpoint, DistroId, DistroSel,
    EndpointMode, GitOps, GitOpsTool, Ingress, KubeProxyMode, Network, Node, NodeOs, NodeRole,
    Provisioning, ServiceLb, ServiceLbKind, Target, Topology, Tuning, TuningProfile,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    DevLocal,
    Homelab,
    ProductionOnPrem,
    CloudHetzner,
}

impl Preset {
    pub const ALL: [Self; 4] = [Self::DevLocal, Self::Homelab, Self::ProductionOnPrem, Self::CloudHetzner];

    pub const fn title(self) -> &'static str {
        match self {
            Self::DevLocal => "Dev / single node",
            Self::Homelab => "Homelab baremetal HA",
            Self::ProductionOnPrem => "Production on-prem",
            Self::CloudHetzner => "Cloud via OpenTofu",
        }
    }

    pub const fn summary(self) -> &'static str {
        match self {
            Self::DevLocal => "k3s, 1 node, Flannel, Traefik. Smallest footprint, HA-ready endpoint.",
            Self::Homelab => "k3s 3 CP + 2 workers, kube-vip VIP, MetalLB L2, Cilium.",
            Self::ProductionOnPrem => "kubeadm 3 CP + 3 workers, Cilium without kube-proxy, Envoy Gateway, Argo CD.",
            Self::CloudHetzner => "RKE2 on Hetzner Cloud created by OpenTofu, cloud LB + CCM.",
        }
    }

    pub fn build(self) -> ClusterSpec {
        match self {
            Self::DevLocal => dev_local(),
            Self::Homelab => homelab(),
            Self::ProductionOnPrem => production(),
            Self::CloudHetzner => cloud_hetzner(),
        }
    }
}

#[inline]
const fn v4(a: u8, b: u8, c: u8, d: u8) -> IpAddr {
    IpAddr::V4(Ipv4Addr::new(a, b, c, d))
}

#[inline]
fn cidr(a: u8, b: u8, c: u8, d: u8, prefix: u8) -> IpNet {
    // Prefix values below are compile-time constants <= 32.
    Ipv4Net::new(Ipv4Addr::new(a, b, c, d), prefix).map_or_else(|_| IpNet::default(), IpNet::V4)
}

/// Builds `n` nodes named `<prefix>-<i>` starting at `192.168.1.<start>`.
fn nodes(prefix: &str, role: NodeRole, start: u8, n: u8, out: &mut Vec<Node>) {
    out.extend((0..n).map(|i| Node {
        name: format!("{prefix}-{}", i + 1),
        address: v4(192, 168, 1, start.saturating_add(i)),
        role,
        ssh_user: "root".to_owned(),
        ssh_key: Some("~/.ssh/id_ed25519".to_owned()),
    }));
}

fn base_network(cni: Cni) -> Network {
    Network {
        cni,
        pod_cidr: cidr(10, 42, 0, 0, 16),
        service_cidr: cidr(10, 43, 0, 0, 16),
        node_cidr: Some(cidr(192, 168, 1, 0, 24)),
        cluster_domain: "cluster.local".to_owned(),
        kube_proxy: KubeProxyMode::Iptables,
        mtu: None,
        dual_stack: false,
    }
}

fn kube_vip_endpoint() -> CpEndpoint {
    CpEndpoint {
        mode: EndpointMode::KubeVip,
        vip: Some(v4(192, 168, 1, 100)),
        interface: Some("eth0".to_owned()),
        dns_name: None,
        port: 6443,
    }
}

fn baremetal_spec(name: &str, distro: DistroId, version: &str, cps: u8, workers: u8, cni: Cni) -> ClusterSpec {
    let mut list = Vec::with_capacity(usize::from(cps) + usize::from(workers));
    nodes("cp", NodeRole::ControlPlane, 11, cps, &mut list);
    nodes("worker", NodeRole::Worker, 21, workers, &mut list);
    ClusterSpec {
        name: name.to_owned(),
        target: Target::Baremetal,
        distro: DistroSel { id: distro, version: version.to_owned() },
        topology: Topology { ha: cps > 1, control_planes: cps, workers, os: NodeOs::Ubuntu2404, nodes: list },
        endpoint: kube_vip_endpoint(),
        service_lb: ServiceLb { kind: ServiceLbKind::None, advertisement: Advertisement::L2, pool: None },
        network: base_network(cni),
        ingress: Ingress::Traefik,
        addons: vec![Addon::MetricsServer, Addon::LocalPath],
        gitops: GitOps::default(),
        tuning: Tuning { profile: TuningProfile::Minimal, node_memory_gib: Some(4) },
    }
}

fn metallb() -> ServiceLb {
    ServiceLb {
        kind: ServiceLbKind::Metallb,
        advertisement: Advertisement::L2,
        pool: Some("192.168.1.240-192.168.1.250".to_owned()),
    }
}

fn dev_local() -> ClusterSpec {
    baremetal_spec("dev", DistroId::K3s, "v1.34.2+k3s1", 1, 0, Cni::Flannel)
}

fn homelab() -> ClusterSpec {
    let mut s = baremetal_spec("homelab", DistroId::K3s, "v1.34.2+k3s1", 3, 2, Cni::Cilium);
    s.service_lb = metallb();
    s.addons.push(Addon::CertManager);
    s
}

fn production() -> ClusterSpec {
    let mut s = baremetal_spec("prod", DistroId::Kubeadm, "v1.34.2", 3, 3, Cni::Cilium);
    s.network.kube_proxy = KubeProxyMode::None;
    s.service_lb = metallb();
    s.ingress = Ingress::EnvoyGateway;
    s.addons = vec![Addon::MetricsServer, Addon::CertManager, Addon::Longhorn, Addon::VictoriaMetrics, Addon::Velero];
    s.gitops = GitOps {
        tool: GitOpsTool::ArgoCd,
        repo_url: "git@github.com:example/prod-gitops.git".to_owned(),
        branch: "main".to_owned(),
        path: "clusters/prod".to_owned(),
    };
    s.tuning = Tuning { profile: TuningProfile::Balanced, node_memory_gib: Some(16) };
    s
}

fn cloud_hetzner() -> ClusterSpec {
    let mut s = baremetal_spec("cloud", DistroId::Rke2, "v1.34.2+rke2r1", 3, 2, Cni::Canal);
    s.topology.nodes.clear();
    s.target = Target::Cloud {
        provider: CloudProvider::Hetzner,
        provisioning: Provisioning::Tofu {
            region: "fsn1".to_owned(),
            cp_size: "cpx21".to_owned(),
            worker_size: "cpx31".to_owned(),
            image: "ubuntu-24.04".to_owned(),
        },
    };
    s.endpoint = CpEndpoint { mode: EndpointMode::CloudLb, vip: None, interface: None, dns_name: None, port: 6443 };
    s.service_lb = ServiceLb { kind: ServiceLbKind::CloudCcm, advertisement: Advertisement::L2, pool: None };
    s.network.node_cidr = Some(cidr(10, 0, 0, 0, 16));
    s.ingress = Ingress::Traefik;
    s
}
