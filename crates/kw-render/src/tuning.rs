//! Tuning tables per profile: sysctl, disabled services, kubelet reservations.

use std::collections::BTreeMap;

use kw_core::{Tuning, TuningProfile};
use serde::Serialize;

type Sysctl = BTreeMap<&'static str, &'static str>;

const COMMON: [(&str, &str); 10] = [
    ("fs.inotify.max_user_watches", "524288"),
    ("fs.inotify.max_user_instances", "8192"),
    ("fs.file-max", "2097152"),
    ("vm.swappiness", "0"),
    ("vm.overcommit_memory", "1"),
    ("vm.max_map_count", "262144"),
    ("kernel.panic", "10"),
    ("kernel.panic_on_oops", "1"),
    ("net.ipv4.ip_local_port_range", "1024 65535"),
    ("net.ipv4.tcp_tw_reuse", "1"),
];

const MINIMAL: [(&str, &str); 4] = [
    ("net.core.somaxconn", "4096"),
    ("net.ipv4.tcp_max_syn_backlog", "4096"),
    ("net.netfilter.nf_conntrack_max", "131072"),
    ("net.core.netdev_max_backlog", "4096"),
];

const BALANCED: [(&str, &str); 7] = [
    ("net.core.somaxconn", "16384"),
    ("net.ipv4.tcp_max_syn_backlog", "16384"),
    ("net.netfilter.nf_conntrack_max", "524288"),
    ("net.core.netdev_max_backlog", "16384"),
    ("net.ipv4.neigh.default.gc_thresh1", "4096"),
    ("net.ipv4.neigh.default.gc_thresh2", "8192"),
    ("net.ipv4.neigh.default.gc_thresh3", "16384"),
];

const THROUGHPUT: [(&str, &str); 11] = [
    ("net.core.somaxconn", "65535"),
    ("net.ipv4.tcp_max_syn_backlog", "65535"),
    ("net.netfilter.nf_conntrack_max", "1048576"),
    ("net.core.netdev_max_backlog", "65536"),
    ("net.ipv4.neigh.default.gc_thresh1", "8192"),
    ("net.ipv4.neigh.default.gc_thresh2", "32768"),
    ("net.ipv4.neigh.default.gc_thresh3", "65536"),
    ("net.core.rmem_max", "16777216"),
    ("net.core.wmem_max", "16777216"),
    ("net.ipv4.tcp_rmem", "4096 87380 16777216"),
    ("net.ipv4.tcp_wmem", "4096 65536 16777216"),
];

pub fn sysctl(profile: TuningProfile) -> Sysctl {
    let extra: &[(&str, &str)] = match profile {
        TuningProfile::Minimal => &MINIMAL,
        TuningProfile::Balanced => &BALANCED,
        TuningProfile::Throughput => &THROUGHPUT,
    };
    COMMON.iter().chain(extra).copied().collect()
}

/// Services stopped on every node (ignored when absent).
pub const DISABLE_SERVICES: &[&str] = &[
    "snapd",
    "snapd.socket",
    "ModemManager",
    "bluetooth",
    "cups",
    "avahi-daemon",
    "multipathd",
    "apport",
    "motd-news.timer",
    "packagekit",
];

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Kubelet {
    pub system_reserved: String,
    pub kube_reserved: String,
    pub eviction_hard: &'static str,
    pub max_pods: u16,
    pub image_gc_high: u8,
    pub image_gc_low: u8,
    pub cpu_manager_policy: &'static str,
}

/// GKE-style memory reservation (MiB) for a node with `gib` GiB RAM:
/// 25% of first 4 GiB, 20% next 4, 10% next 8, 6% next 112, 2% above.
pub fn reserved_mib(gib: u32) -> u32 {
    const TIERS: [(u32, u32); 5] = [(4, 25), (4, 20), (8, 10), (112, 6), (u32::MAX, 2)];
    let mut left = gib;
    TIERS
        .iter()
        .map(|&(size, pct)| {
            let take = left.min(size);
            left -= take;
            take * 1024 * pct / 100
        })
        .sum()
}

pub fn kubelet(t: &Tuning) -> Kubelet {
    let gib = u32::from(t.node_memory_gib.unwrap_or(4)).max(1);
    let total = reserved_mib(gib);
    let (kube, system) = (total * 6 / 10, total * 4 / 10);
    let (cpu_k, cpu_s, eviction, max_pods, gc_high, gc_low, cpu_policy) = match t.profile {
        TuningProfile::Minimal => ("100m", "100m", "memory.available<100Mi,nodefs.available<10%,imagefs.available<15%", 110, 75, 60, "none"),
        TuningProfile::Balanced => ("200m", "150m", "memory.available<200Mi,nodefs.available<10%,imagefs.available<15%", 110, 85, 75, "none"),
        TuningProfile::Throughput => ("500m", "250m", "memory.available<500Mi,nodefs.available<10%,imagefs.available<15%", 200, 90, 85, "static"),
    };
    Kubelet {
        system_reserved: format!("cpu={cpu_s},memory={system}Mi"),
        kube_reserved: format!("cpu={cpu_k},memory={kube}Mi"),
        eviction_hard: eviction,
        max_pods,
        image_gc_high: gc_high,
        image_gc_low: gc_low,
        cpu_manager_policy: cpu_policy,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gke_reservation() {
        assert_eq!(reserved_mib(4), 1024);
        assert_eq!(reserved_mib(8), 1024 + 819);
        assert_eq!(reserved_mib(16), 1024 + 819 + 819);
    }

    #[test]
    fn profile_overrides_common() {
        let s = sysctl(TuningProfile::Throughput);
        assert_eq!(s["net.core.somaxconn"], "65535");
        assert_eq!(s["vm.swappiness"], "0");
    }
}
