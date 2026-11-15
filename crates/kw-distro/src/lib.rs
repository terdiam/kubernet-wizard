//! Distro plugins. Static enum dispatch: no `dyn`, no heap.
//!
//! Add a distro: unit struct implementing [`Distro`], a [`DistroKind`]
//! variant, and its arm in each `match` below (see `kw-add-distro` skill).

mod data;

use std::collections::BTreeMap;
use std::sync::OnceLock;

use kw_core::{Capabilities, ClusterSpec, DistroId, Phase};
use serde::Serialize;

pub use data::{K3s, Kubeadm, Planned, Rke2};

/// Static variables handed to templates for the selected distro.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct DistroVars {
    pub id: &'static str,
    /// Ansible role that installs the distro.
    pub role: &'static str,
    /// Kubeconfig path on control-plane nodes.
    pub kubeconfig: &'static str,
    /// Directory where the distro auto-applies manifests (None = kubectl apply).
    pub manifests_dir: Option<&'static str>,
    /// Static pod dir for kube-vip (kubeadm) — None if manifests_dir is used.
    pub static_pods_dir: Option<&'static str>,
    /// Distro embeds its own container runtime.
    pub bundled_runtime: bool,
    /// kubectl binary path on nodes.
    pub kubectl: &'static str,
}

pub trait Distro {
    fn id(&self) -> DistroId;
    fn capabilities(&self) -> Capabilities;
    fn vars(&self) -> DistroVars;
    /// Distro-specific HA upgrade notes appended to `kw_core::ha::HA_STEPS`.
    fn ha_notes(&self) -> &'static [&'static str];
    /// Short description for UI cards.
    fn description(&self) -> &'static str;

    #[inline]
    fn label(&self) -> &'static str {
        self.id().label()
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.capabilities().is_ready()
    }

    /// Default version = first in the tested catalog.
    #[inline]
    fn default_version(&self) -> &'static str {
        self.capabilities().versions.first().copied().unwrap_or("")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DistroKind {
    Kubeadm(Kubeadm),
    K3s(K3s),
    Rke2(Rke2),
    Planned(Planned),
}

macro_rules! dispatch {
    ($self:ident, $d:ident => $e:expr) => {
        match $self {
            DistroKind::Kubeadm($d) => $e,
            DistroKind::K3s($d) => $e,
            DistroKind::Rke2($d) => $e,
            DistroKind::Planned($d) => $e,
        }
    };
}

impl Distro for DistroKind {
    #[inline]
    fn id(&self) -> DistroId {
        dispatch!(self, d => d.id())
    }
    #[inline]
    fn capabilities(&self) -> Capabilities {
        let mut caps = dispatch!(self, d => d.capabilities());
        if caps.is_ready()
            && let Some(v) = CATALOG.get().and_then(|c| c.get(&self.id()))
        {
            caps.versions = v;
        }
        caps
    }
    #[inline]
    fn vars(&self) -> DistroVars {
        dispatch!(self, d => d.vars())
    }
    #[inline]
    fn ha_notes(&self) -> &'static [&'static str] {
        dispatch!(self, d => d.ha_notes())
    }
    #[inline]
    fn description(&self) -> &'static str {
        dispatch!(self, d => d.description())
    }
}

type VersionMap = BTreeMap<DistroId, &'static [&'static str]>;

/// Version catalog override (from the signed online catalog). Set once.
static CATALOG: OnceLock<VersionMap> = OnceLock::new();

/// Installs a newer tested-version catalog. Only the first call wins; strings
/// are leaked once per process (a few hundred bytes), keeping `Capabilities`
/// `Copy` and allocation-free on every read.
pub fn set_catalog<I, V>(entries: I) -> bool
where
    I: IntoIterator<Item = (DistroId, V)>,
    V: IntoIterator<Item = String>,
{
    let map: VersionMap = entries
        .into_iter()
        .filter_map(|(id, versions)| {
            let leaked: Vec<&'static str> = versions.into_iter().map(|v| &*Box::leak(v.into_boxed_str())).collect();
            (!leaked.is_empty()).then(|| (id, &*Box::leak(leaked.into_boxed_slice())))
        })
        .collect();
    CATALOG.set(map).is_ok()
}

/// All distros in display order (ready first, then planned by phase).
pub const REGISTRY: [DistroKind; 12] = [
    DistroKind::Kubeadm(Kubeadm),
    DistroKind::K3s(K3s),
    DistroKind::Rke2(Rke2),
    DistroKind::Planned(Planned::new(DistroId::K0s, 1, "Single binary, zero dependencies. Next in phase 1.")),
    DistroKind::Planned(Planned::new(DistroId::Microk8s, 2, "Snap based, Canonical. Addon system.")),
    DistroKind::Planned(Planned::new(DistroId::Charmed, 2, "Juju operated Canonical Kubernetes.")),
    DistroKind::Planned(Planned::new(DistroId::Kubesphere, 2, "KubeKey installer + KubeSphere platform.")),
    DistroKind::Planned(Planned::new(DistroId::Deckhouse, 2, "NoOps platform via dhctl.")),
    DistroKind::Planned(Planned::new(DistroId::Kfd, 2, "SIGHUP Fury distribution via furyctl.")),
    DistroKind::Planned(Planned::new(DistroId::Typhoon, 3, "Minimal Terraform based distro.")),
    DistroKind::Planned(Planned::new(DistroId::Metalk8s, 3, "Scality on-prem distribution.")),
    DistroKind::Planned(Planned::new(DistroId::Kubecube, 3, "Multi-tenant management platform.")),
];

/// Look up by id. Always succeeds since every id is registered.
#[inline]
pub fn get(id: DistroId) -> DistroKind {
    REGISTRY
        .iter()
        .copied()
        .find(|d| d.id() == id)
        .unwrap_or(DistroKind::Planned(Planned::new(id, 3, "")))
}

/// Capabilities of the spec's distro.
#[inline]
pub fn caps_for(spec: &ClusterSpec) -> Capabilities {
    get(spec.distro.id).capabilities()
}

/// Full validation (core rules + distro capabilities).
#[inline]
pub fn validate(spec: &ClusterSpec) -> kw_core::Report {
    kw_core::validate(spec, &caps_for(spec))
}

/// Phase label for UI badges.
#[inline]
pub const fn phase_label(p: Phase) -> &'static str {
    match p {
        Phase::Ready => "Ready",
        Phase::Planned(1) => "Soon (phase 1)",
        Phase::Planned(2) => "Planned (phase 2)",
        Phase::Planned(_) => "Planned (phase 3)",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_id_registered_once() {
        for d in REGISTRY {
            assert_eq!(REGISTRY.iter().filter(|o| o.id() == d.id()).count(), 1);
            assert_eq!(get(d.id()).id(), d.id());
        }
    }

    #[test]
    fn ready_distros_have_versions() {
        for d in REGISTRY.iter().filter(|d| d.is_ready()) {
            assert!(!d.default_version().is_empty(), "{:?}", d.id());
        }
    }

    #[test]
    fn catalog_overrides_ready_versions_only() {
        assert!(set_catalog([
            (DistroId::K3s, vec!["v9.9.9+k3s1".to_owned()]),
            (DistroId::Microk8s, vec!["1.0".to_owned()]),
        ]));
        assert_eq!(get(DistroId::K3s).capabilities().versions, &["v9.9.9+k3s1"]);
        assert_eq!(get(DistroId::Microk8s).capabilities().versions, &[] as &[&str]);
        assert!(!set_catalog([(DistroId::K3s, vec!["x".to_owned()])]), "second set ignored");
    }

    #[test]
    fn presets_validate_with_real_caps() {
        for p in kw_core::presets::Preset::ALL {
            let r = validate(&p.build());
            assert!(!r.has_errors(), "{p:?}: {:?}", r.issues);
        }
    }
}
