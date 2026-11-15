//! Context-panel content: what an option does, trade-offs, when to use it.

use kw_core::{Cni, DistroId, EndpointMode, GitOpsTool, Ingress, ServiceLbKind, Step, TuningProfile};
use kw_distro::Distro;

use crate::docs::DocPage;

/// Item currently hovered/focused in the wizard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Topic {
    Step(Step),
    Distro(DistroId),
    Cni(Cni),
    Endpoint(EndpointMode),
    ServiceLb(ServiceLbKind),
    Ingress(Ingress),
    GitOps(GitOpsTool),
    Tuning(TuningProfile),
}

pub struct Help {
    pub title: &'static str,
    pub body: &'static str,
    pub doc: DocPage,
}

#[inline]
const fn h(title: &'static str, body: &'static str, doc: DocPage) -> Help {
    Help { title, body, doc }
}

pub const fn doc_for_step(step: Step) -> DocPage {
    match step {
        Step::Topology => DocPage::HighAvailability,
        Step::Endpoint => DocPage::EndpointLb,
        Step::Network => DocPage::Network,
        Step::Ingress | Step::GitOps | Step::Features => DocPage::IngressGitOps,
        Step::Tuning => DocPage::Tuning,
        Step::Deploy => DocPage::Troubleshooting,
        _ => DocPage::GettingStarted,
    }
}

pub fn topic(t: Topic) -> Help {
    use DocPage as D;
    match t {
        Topic::Step(s) => step(s),
        Topic::Distro(id) => h(id.label(), kw_distro::get(id).description(), D::GettingStarted),
        Topic::Cni(c) => match c {
            Cni::Cilium => h("Cilium", "eBPF datapath. Can replace kube-proxy (lower CPU), Gateway API, network policy. Uses ~300 MiB per node.", D::Network),
            Cni::Calico => h("Calico", "Mature network policy and optional BGP. Good default for kubeadm when eBPF is not wanted.", D::Network),
            Cni::Flannel => h("Flannel", "Simplest and lightest overlay (~50 MiB). No NetworkPolicy. Built into k3s.", D::Network),
            Cni::Canal => h("Canal", "Flannel networking + Calico policy. Default for RKE2.", D::Network),
        },
        Topic::Endpoint(m) => match m {
            EndpointMode::KubeVip => h("kube-vip", "Virtual IP for the API server held by a control-plane pod (ARP). No extra packages. Required even for one control plane so you can upgrade to HA later.", D::EndpointLb),
            EndpointMode::KeepalivedHaproxy => h("keepalived + HAProxy", "Classic VRRP VIP with HAProxy health checks on each control plane. API is exposed on port 8443.", D::EndpointLb),
            EndpointMode::CloudLb => h("Cloud load balancer", "Managed TCP load balancer created by OpenTofu in front of the control planes. Cloud targets only.", D::EndpointLb),
        },
        Topic::ServiceLb(k) => match k {
            ServiceLbKind::Metallb => h("MetalLB", "Gives LoadBalancer Services real IPs on baremetal. L2 mode works on any flat network; BGP for routers.", D::EndpointLb),
            ServiceLbKind::KubeVip => h("kube-vip cloud provider", "Reuses kube-vip for Service IPs from a range. One component for VIP + services.", D::EndpointLb),
            ServiceLbKind::CloudCcm => h("Cloud controller", "Uses the provider's native load balancers for Services.", D::EndpointLb),
            ServiceLbKind::None => h("None", "Services of type LoadBalancer stay pending. Use NodePort / ingress only.", D::EndpointLb),
        },
        Topic::Ingress(i) => match i {
            Ingress::Traefik => h("Traefik", "Lightweight ingress controller, bundled with k3s.", D::IngressGitOps),
            Ingress::EnvoyGateway => h("Envoy Gateway", "Gateway API implementation built on Envoy. Recommended for new production clusters.", D::IngressGitOps),
            Ingress::CiliumGateway => h("Cilium Gateway API", "Gateway API served by Cilium itself; no extra controller pods.", D::IngressGitOps),
            Ingress::NginxGatewayFabric => h("NGINX Gateway Fabric", "Gateway API by NGINX. Successor path for ingress-nginx users.", D::IngressGitOps),
            Ingress::HaproxyIngress => h("HAProxy Ingress", "High performance Ingress controller by HAProxy Technologies.", D::IngressGitOps),
            Ingress::Contour => h("Contour", "Envoy-based Ingress/HTTPProxy controller (CNCF).", D::IngressGitOps),
            Ingress::None => h("None", "No ingress controller. Add one later via GitOps.", D::IngressGitOps),
        },
        Topic::GitOps(g) => match g {
            GitOpsTool::ArgoCd => h("Argo CD", "Pull-based GitOps with a web UI and app-of-apps. Add-ons are written as Applications to your repo.", D::IngressGitOps),
            GitOpsTool::Flux => h("Flux", "Lightweight GitOps controllers without UI. Add-ons become HelmReleases.", D::IngressGitOps),
            GitOpsTool::None => h("None", "Add-ons are installed once by Ansible with Helm.", D::IngressGitOps),
        },
        Topic::Tuning(p) => match p {
            TuningProfile::Minimal => h("Minimal footprint", "Small reservations, aggressive image GC, Hubble off, conservative network buffers. Best for homelab and small VMs.", D::Tuning),
            TuningProfile::Balanced => h("Balanced", "Production defaults: larger conntrack/backlog, safer reservations.", D::Tuning),
            TuningProfile::Throughput => h("Throughput", "Large TCP buffers, performance CPU governor, static CPU manager. Uses more resources.", D::Tuning),
        },
    }
}

fn step(s: Step) -> Help {
    let doc = doc_for_step(s);
    match s {
        Step::Project => h("Project", "Name your cluster and choose where the generated OpenTofu/Ansible project is written. The spec is saved as cluster.yaml.", doc),
        Step::Target => h("Target", "Baremetal uses servers you list. Cloud can either create VMs with OpenTofu or use VMs you already have.", doc),
        Step::Distro => h("Distribution", "Ready distros are fully automated. Planned ones are shown so you know what is coming.", doc),
        Step::Topology => h("Topology", "HA needs 3 or 5 control planes. A single control plane is still built HA-ready (stable endpoint + etcd).", doc),
        Step::Endpoint => h("Endpoint & LB", "The control-plane endpoint is the stable API address. The Service LB gives LoadBalancer Services external IPs.", doc),
        Step::Network => h("Network", "Choose the CNI and address ranges. Pod/Service CIDRs must not overlap the node subnet.", doc),
        Step::Ingress => h("Ingress / Gateway", "How HTTP traffic enters the cluster. Gateway API options are the modern choice.", doc),
        Step::Features => h("Features", "Optional add-ons. Each adds resource usage; the footprint estimate updates live.", doc),
        Step::GitOps => h("GitOps", "Let Argo CD or Flux manage add-ons from a Git repository after bootstrap.", doc),
        Step::Tuning => h("Tuning", "OS, containerd and kubelet tuning for performance with minimal CPU/RAM overhead.", doc),
        Step::Review => h("Review", "Every file that will be generated. Nothing runs until you deploy.", doc),
        Step::Deploy => h("Deploy", "Generate files, then run OpenTofu/Ansible with live logs. Demo mode simulates the run.", doc),
    }
}
