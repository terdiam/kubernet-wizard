//! Per-distro static data.

use kw_core::{Capabilities, Cni, DistroId, Ingress, KubeProxyMode, NodeOs, Phase};

use crate::{Distro, DistroVars};

const ALL_OS: &[NodeOs] = NodeOs::ALL;
const ALL_KP: &[KubeProxyMode] = KubeProxyMode::ALL;

// ---------------------------------------------------------------- kubeadm

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Kubeadm;

impl Distro for Kubeadm {
    fn id(&self) -> DistroId {
        DistroId::Kubeadm
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            phase: Phase::Ready,
            versions: &["v1.34.2", "v1.33.6", "v1.32.10"],
            cnis: &[Cni::Cilium, Cni::Calico, Cni::Flannel],
            kube_proxy: ALL_KP,
            os: ALL_OS,
            bundled_ingress: None,
        }
    }
    fn vars(&self) -> DistroVars {
        DistroVars {
            id: "kubeadm",
            role: "distro_kubeadm",
            kubeconfig: "/etc/kubernetes/admin.conf",
            manifests_dir: None,
            static_pods_dir: Some("/etc/kubernetes/manifests"),
            bundled_runtime: false,
            kubectl: "/usr/bin/kubectl",
        }
    }
    fn ha_notes(&self) -> &'static [&'static str] {
        &["kubeadm init phase upload-certs --upload-certs on first CP", "kubeadm join --control-plane --certificate-key"]
    }
    fn description(&self) -> &'static str {
        "Upstream Kubernetes. Most flexible, standard for production."
    }
}

// ---------------------------------------------------------------- k3s

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct K3s;

impl Distro for K3s {
    fn id(&self) -> DistroId {
        DistroId::K3s
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            phase: Phase::Ready,
            versions: &["v1.34.2+k3s1", "v1.33.6+k3s1", "v1.32.10+k3s1"],
            cnis: &[Cni::Flannel, Cni::Cilium, Cni::Calico],
            kube_proxy: ALL_KP,
            os: ALL_OS,
            bundled_ingress: Some(Ingress::Traefik),
        }
    }
    fn vars(&self) -> DistroVars {
        DistroVars {
            id: "k3s",
            role: "distro_k3s",
            kubeconfig: "/etc/rancher/k3s/k3s.yaml",
            manifests_dir: Some("/var/lib/rancher/k3s/server/manifests"),
            static_pods_dir: None,
            bundled_runtime: true,
            kubectl: "/usr/local/bin/kubectl",
        }
    }
    fn ha_notes(&self) -> &'static [&'static str] {
        &["First server runs with --cluster-init (embedded etcd)", "New servers join with --server https://<endpoint>:6443"]
    }
    fn description(&self) -> &'static str {
        "Lightweight certified Kubernetes. Lowest CPU/RAM, embedded etcd HA."
    }
}

// ---------------------------------------------------------------- RKE2

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rke2;

impl Distro for Rke2 {
    fn id(&self) -> DistroId {
        DistroId::Rke2
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            phase: Phase::Ready,
            versions: &["v1.34.2+rke2r1", "v1.33.6+rke2r1", "v1.32.10+rke2r1"],
            cnis: &[Cni::Canal, Cni::Cilium, Cni::Calico, Cni::Flannel],
            kube_proxy: &[KubeProxyMode::Iptables, KubeProxyMode::Ipvs, KubeProxyMode::None],
            os: ALL_OS,
            bundled_ingress: None,
        }
    }
    fn vars(&self) -> DistroVars {
        DistroVars {
            id: "rke2",
            role: "distro_rke2",
            kubeconfig: "/etc/rancher/rke2/rke2.yaml",
            manifests_dir: Some("/var/lib/rancher/rke2/server/manifests"),
            static_pods_dir: None,
            bundled_runtime: true,
            kubectl: "/var/lib/rancher/rke2/bin/kubectl",
        }
    }
    fn ha_notes(&self) -> &'static [&'static str] {
        &["New servers join via server: https://<endpoint>:9345 with cluster token"]
    }
    fn description(&self) -> &'static str {
        "Security hardened (CIS) distro by SUSE. etcd HA, FIPS ready."
    }
}

// ---------------------------------------------------------------- planned

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Planned {
    id: DistroId,
    phase: u8,
    description: &'static str,
}

impl Planned {
    pub const fn new(id: DistroId, phase: u8, description: &'static str) -> Self {
        Self { id, phase, description }
    }
}

impl Distro for Planned {
    fn id(&self) -> DistroId {
        self.id
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities { phase: Phase::Planned(self.phase), ..Capabilities::PLANNED_EMPTY }
    }
    fn vars(&self) -> DistroVars {
        DistroVars {
            id: "planned",
            role: "",
            kubeconfig: "",
            manifests_dir: None,
            static_pods_dir: None,
            bundled_runtime: false,
            kubectl: "kubectl",
        }
    }
    fn ha_notes(&self) -> &'static [&'static str] {
        &[]
    }
    fn description(&self) -> &'static str {
        self.description
    }
}
