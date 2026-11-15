//! Compatibility rules. Each fn returns `Some(reason)` when an option is
//! unavailable, so the UI can disable it with a tooltip and the validator can
//! reuse the very same text.

use crate::caps::Capabilities;
use crate::spec::{
    ClusterSpec, Cni, EndpointMode, Ingress, KubeProxyMode, NodeOs, ServiceLbKind,
};

pub type Reason = Option<&'static str>;

#[inline]
pub fn cni(caps: &Capabilities, cni: Cni) -> Reason {
    (!caps.supports_cni(cni)).then_some("Not supported by the selected distro")
}

#[inline]
pub fn os(caps: &Capabilities, os: NodeOs) -> Reason {
    (!caps.supports_os(os)).then_some("Not supported by the selected distro")
}

pub fn kube_proxy(caps: &Capabilities, spec_cni: Cni, mode: KubeProxyMode) -> Reason {
    if !caps.supports_kube_proxy(mode) {
        return Some("Not supported by the selected distro");
    }
    (mode == KubeProxyMode::None && spec_cni != Cni::Cilium)
        .then_some("Disabling kube-proxy requires Cilium (eBPF kube-proxy replacement)")
}

#[inline]
pub fn endpoint(cloud_lb: bool, mode: EndpointMode) -> Reason {
    (mode == EndpointMode::CloudLb && !cloud_lb)
        .then_some("Cloud load balancer needs a cloud provider with managed LBs (not baremetal or Proxmox)")
}

#[inline]
pub fn service_lb(cloud_lb: bool, kind: ServiceLbKind) -> Reason {
    (kind == ServiceLbKind::CloudCcm && !cloud_lb)
        .then_some("Cloud controller manager needs a cloud provider with managed LBs (not baremetal or Proxmox)")
}

#[inline]
pub fn ingress(spec_cni: Cni, ingress: Ingress) -> Reason {
    (ingress == Ingress::CiliumGateway && spec_cni != Cni::Cilium)
        .then_some("Cilium Gateway API requires Cilium as CNI")
}

/// Convenience: check whole spec selections (used by validator).
pub(crate) fn spec_reasons(spec: &ClusterSpec, caps: &Capabilities) -> [(crate::Field, Reason); 7] {
    use crate::Field;
    let cloud = spec.has_cloud_lb();
    let net = &spec.network;
    [
        (Field::Cni, cni(caps, net.cni)),
        (Field::Os, os(caps, spec.topology.os)),
        (Field::KubeProxy, kube_proxy(caps, net.cni, net.kube_proxy)),
        (Field::EndpointMode, endpoint(cloud, spec.endpoint.mode)),
        (Field::ServiceLb, service_lb(cloud, spec.service_lb.kind)),
        (Field::Ingress, ingress(net.cni, spec.ingress)),
        (
            Field::Distro,
            (!caps.is_ready()).then_some("This distro is planned for a later release"),
        ),
    ]
}
