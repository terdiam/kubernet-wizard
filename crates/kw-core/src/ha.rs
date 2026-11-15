//! Single control plane → HA upgrade planning.

use crate::spec::{ClusterSpec, EndpointMode, Node, NodeRole};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum HaError {
    #[error("cluster is already HA")]
    AlreadyHa,
    #[error("no stable control-plane endpoint (VIP or cloud LB) configured; cluster cannot be upgraded in place")]
    NoStableEndpoint,
    #[error("need {need} new control-plane node(s), got {got}")]
    WrongCount { need: u8, got: usize },
    #[error("new node {0} must have role control_plane")]
    WrongRole(String),
    #[error("node address {0} already in inventory")]
    Duplicate(String),
}

/// Generic, distro-independent upgrade steps (distro adds its own details).
pub const HA_STEPS: [&str; 6] = [
    "Verify endpoint (VIP / LB) answers on the API port",
    "Prepare new nodes (prereq, tuning, runtime)",
    "Join new control planes one by one",
    "Wait for etcd to report 3 healthy members",
    "Scale control-plane endpoint backends (kube-vip / LB)",
    "Refresh kubeconfig and run smoke test",
];

/// Returns updated spec with the new control planes appended.
pub fn plan_upgrade(spec: &ClusterSpec, new_cps: &[Node], target_cps: u8) -> Result<ClusterSpec, HaError> {
    if spec.topology.ha {
        return Err(HaError::AlreadyHa);
    }
    let stable = match spec.endpoint.mode {
        EndpointMode::KubeVip | EndpointMode::KeepalivedHaproxy => spec.endpoint.vip.is_some(),
        EndpointMode::CloudLb => true,
    };
    if !stable {
        return Err(HaError::NoStableEndpoint);
    }
    let need = target_cps.saturating_sub(spec.topology.control_planes);
    if new_cps.len() != usize::from(need) || !matches!(target_cps, 3 | 5) {
        return Err(HaError::WrongCount { need, got: new_cps.len() });
    }
    if let Some(n) = new_cps.iter().find(|n| n.role != NodeRole::ControlPlane) {
        return Err(HaError::WrongRole(n.name.clone()));
    }
    if let Some(n) = new_cps.iter().find(|n| spec.topology.nodes.iter().any(|e| e.address == n.address)) {
        return Err(HaError::Duplicate(n.address.to_string()));
    }

    let mut next = spec.clone();
    next.topology.ha = true;
    next.topology.control_planes = target_cps;
    next.topology.nodes.reserve(new_cps.len());
    next.topology.nodes.extend_from_slice(new_cps);
    Ok(next)
}
