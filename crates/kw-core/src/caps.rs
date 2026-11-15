//! Distro capability data used by validation. Static, zero-allocation.

use crate::spec::{Cni, Ingress, KubeProxyMode, NodeOs};

/// Release phase of a distro integration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Fully supported now.
    Ready,
    /// Planned for a later release (shown disabled in UI).
    Planned(u8),
}

#[derive(Debug, Clone, Copy)]
pub struct Capabilities {
    pub phase: Phase,
    pub versions: &'static [&'static str],
    pub cnis: &'static [Cni],
    pub kube_proxy: &'static [KubeProxyMode],
    pub os: &'static [NodeOs],
    /// Ingress shipped by the distro itself (disabled when user picks another).
    pub bundled_ingress: Option<Ingress>,
}

impl Capabilities {
    pub const PLANNED_EMPTY: Self = Self {
        phase: Phase::Planned(2),
        versions: &[],
        cnis: &[],
        kube_proxy: &[],
        os: &[],
        bundled_ingress: None,
    };

    #[inline]
    pub fn supports_cni(&self, cni: Cni) -> bool {
        self.cnis.contains(&cni)
    }

    #[inline]
    pub fn supports_os(&self, os: NodeOs) -> bool {
        self.os.contains(&os)
    }

    #[inline]
    pub fn supports_kube_proxy(&self, mode: KubeProxyMode) -> bool {
        self.kube_proxy.contains(&mode)
    }

    #[inline]
    pub const fn is_ready(&self) -> bool {
        matches!(self.phase, Phase::Ready)
    }
}
