//! Rough system overhead estimate per node (MiB / millicores). Static table,
//! intended as UI guidance only.

use crate::spec::{Addon, ClusterSpec, Cni, DistroId, Ingress};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Footprint {
    pub mem_mib: u32,
    pub cpu_m: u32,
}

impl core::ops::Add for Footprint {
    type Output = Self;
    #[inline]
    fn add(self, o: Self) -> Self {
        Self { mem_mib: self.mem_mib + o.mem_mib, cpu_m: self.cpu_m + o.cpu_m }
    }
}

#[inline]
const fn fp(mem_mib: u32, cpu_m: u32) -> Footprint {
    Footprint { mem_mib, cpu_m }
}

const fn distro_cp(id: DistroId) -> Footprint {
    match id {
        DistroId::K3s | DistroId::K0s => fp(600, 250),
        DistroId::Rke2 => fp(1300, 500),
        _ => fp(1100, 450),
    }
}

const fn distro_worker(id: DistroId) -> Footprint {
    match id {
        DistroId::K3s | DistroId::K0s => fp(200, 80),
        _ => fp(300, 120),
    }
}

const fn cni(c: Cni) -> Footprint {
    match c {
        Cni::Cilium => fp(300, 100),
        Cni::Calico | Cni::Canal => fp(200, 80),
        Cni::Flannel => fp(50, 20),
    }
}

const fn ingress(i: Ingress) -> Footprint {
    match i {
        Ingress::None | Ingress::CiliumGateway => fp(0, 0),
        Ingress::EnvoyGateway => fp(250, 100),
        _ => fp(120, 50),
    }
}

const fn addon(a: Addon) -> Footprint {
    match a {
        Addon::KubePrometheusStack => fp(2000, 500),
        Addon::RookCeph => fp(3000, 1000),
        Addon::Longhorn => fp(500, 200),
        Addon::VictoriaMetrics | Addon::Loki => fp(400, 150),
        Addon::Openebs | Addon::Velero | Addon::Kyverno => fp(250, 100),
        Addon::CertManager | Addon::ExternalSecrets | Addon::Vcluster => fp(150, 50),
        _ => fp(60, 20),
    }
}

/// (control-plane node, worker node) overhead. Cluster add-ons are spread
/// across workers (or CPs when there are no workers).
pub fn estimate(spec: &ClusterSpec) -> (Footprint, Footprint) {
    let d = spec.distro.id;
    let net = cni(spec.network.cni);
    let cluster_addons = spec.addons.iter().fold(ingress(spec.ingress), |acc, &a| acc + addon(a));
    let carriers = u32::from(spec.topology.workers.max(1));
    let share = fp(cluster_addons.mem_mib / carriers, cluster_addons.cpu_m / carriers);

    let mut cp = distro_cp(d) + net;
    let mut worker = distro_worker(d) + net;
    if spec.topology.workers == 0 {
        cp = cp + share;
    } else {
        worker = worker + share;
    }
    (cp, worker)
}
