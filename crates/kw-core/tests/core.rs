use kw_core::caps::{Capabilities, Phase};
use kw_core::presets::Preset;
use kw_core::*;

const CAPS: Capabilities = Capabilities {
    phase: Phase::Ready,
    versions: &["v1.34.2+k3s1", "v1.34.2", "v1.34.2+rke2r1"],
    cnis: &[Cni::Cilium, Cni::Calico, Cni::Flannel, Cni::Canal],
    kube_proxy: &[KubeProxyMode::Iptables, KubeProxyMode::Ipvs, KubeProxyMode::Nftables, KubeProxyMode::None],
    os: &[NodeOs::Ubuntu2204, NodeOs::Ubuntu2404, NodeOs::Ubuntu2604, NodeOs::Debian12, NodeOs::Debian13, NodeOs::Rhel9],
    bundled_ingress: None,
};

fn errors(spec: &ClusterSpec) -> Vec<Field> {
    validate(spec, &CAPS)
        .issues
        .into_iter()
        .filter(|i| i.severity == Severity::Error)
        .map(|i| i.field)
        .collect()
}

#[test]
fn presets_are_valid() {
    for p in Preset::ALL {
        let s = p.build();
        assert!(errors(&s).is_empty(), "{p:?}: {:?}", validate(&s, &CAPS).issues);
    }
}

#[test]
fn yaml_roundtrip() {
    let s = Preset::ProductionOnPrem.build();
    let back = ClusterSpec::from_yaml(&s.to_yaml().unwrap()).unwrap();
    assert_eq!(s, back);
}

#[test]
fn cidr_overlap_detected() {
    let mut s = Preset::Homelab.build();
    s.network.service_cidr = "10.42.128.0/17".parse().unwrap();
    assert!(errors(&s).contains(&Field::ServiceCidr));
    s.network.service_cidr = "10.43.0.0/16".parse().unwrap();
    s.network.pod_cidr = "192.168.0.0/16".parse().unwrap();
    assert!(errors(&s).contains(&Field::PodCidr));
}

#[test]
fn vip_required_even_single_cp() {
    let mut s = Preset::DevLocal.build();
    s.endpoint.vip = None;
    assert!(errors(&s).contains(&Field::Vip));
}

#[test]
fn vip_outside_subnet() {
    let mut s = Preset::DevLocal.build();
    s.endpoint.vip = Some("10.9.9.9".parse().unwrap());
    assert!(errors(&s).contains(&Field::Vip));
}

#[test]
fn kube_proxy_none_needs_cilium() {
    let mut s = Preset::Homelab.build();
    s.network.cni = Cni::Calico;
    s.network.kube_proxy = KubeProxyMode::None;
    assert!(errors(&s).contains(&Field::KubeProxy));
}

#[test]
fn ha_needs_odd_cp_and_inventory_matches() {
    let mut s = Preset::Homelab.build();
    s.topology.control_planes = 2;
    let e = errors(&s);
    assert!(e.contains(&Field::ControlPlanes));
    assert!(e.contains(&Field::Nodes));
}

#[test]
fn cloud_lb_rejected_on_baremetal() {
    let mut s = Preset::Homelab.build();
    s.endpoint.mode = EndpointMode::CloudLb;
    assert!(errors(&s).contains(&Field::EndpointMode));
}

#[test]
fn pool_overlapping_vip() {
    let mut s = Preset::Homelab.build();
    s.service_lb.pool = Some("192.168.1.90-192.168.1.110".into());
    assert!(errors(&s).contains(&Field::LbPool));
    assert_eq!(validate::parse_pool("10.0.0.0/30").map(|p| p.1.to_string()), Some("10.0.0.3".into()));
    assert!(validate::parse_pool("garbage").is_none());
}

#[test]
fn cluster_dns_is_tenth() {
    let s = Preset::DevLocal.build();
    assert_eq!(s.network.cluster_dns().unwrap().to_string(), "10.43.0.10");
}

#[test]
fn ha_upgrade_plan() {
    let s = Preset::DevLocal.build();
    let mk = |i: u8| Node {
        name: format!("cp-{i}"),
        address: format!("192.168.1.{}", 11 + i).parse().unwrap(),
        role: NodeRole::ControlPlane,
        ssh_user: "root".into(),
        ssh_key: None,
    };
    let new = [mk(1), mk(2)];
    let up = ha::plan_upgrade(&s, &new, 3).unwrap();
    assert!(up.topology.ha);
    assert_eq!(up.nodes_with_role(NodeRole::ControlPlane).count(), 3);
    assert_eq!(errors(&up), Vec::<Field>::new());

    assert_eq!(ha::plan_upgrade(&s, &new[..1], 3), Err(ha::HaError::WrongCount { need: 2, got: 1 }));
    let mut no_vip = s;
    no_vip.endpoint.vip = None;
    assert_eq!(ha::plan_upgrade(&no_vip, &new, 3), Err(ha::HaError::NoStableEndpoint));
}

#[test]
fn footprint_minimal_is_small() {
    let (cp, _) = resources::estimate(&Preset::DevLocal.build());
    assert!(cp.mem_mib < 1000, "{cp:?}");
}

#[test]
fn proxmox_rules() {
    use kw_core::{CloudProvider, EndpointMode, Field, Provisioning, Target, presets::Preset};
    let mut s = Preset::CloudHetzner.build();
    s.target = Target::Cloud { provider: CloudProvider::Proxmox, provisioning: CloudProvider::Proxmox.tofu_defaults().provisioning() };
    let r = validate(&s, &CAPS);
    assert!(r.issues.iter().any(|i| i.field == Field::EndpointMode), "cloud LB rejected on Proxmox");
    assert!(!r.issues.iter().any(|i| i.field == Field::Provisioning), "defaults are valid");

    s.target = Target::Cloud {
        provider: CloudProvider::Proxmox,
        provisioning: Provisioning::Tofu { region: "pve".into(), cp_size: "4gb".into(), worker_size: "2x4096".into(), image: "ubuntu".into() },
    };
    let r = validate(&s, &CAPS);
    assert_eq!(r.issues.iter().filter(|i| i.field == Field::Provisioning).count(), 2);

    s.target = Target::Cloud { provider: CloudProvider::Aws, provisioning: Provisioning::ExistingVms };
    s.endpoint.mode = EndpointMode::CloudLb;
    s.endpoint.dns_name = None;
    assert!(validate(&s, &CAPS).issues.iter().any(|i| i.field == Field::EndpointMode), "existing VMs need LB address");
}

#[test]
fn no_vip_on_cloud_dns_endpoint_instead() {
    use kw_core::{CloudProvider, EndpointMode, Field, Target, presets::Preset};
    let mut s = Preset::CloudHetzner.build();
    s.endpoint.mode = EndpointMode::KubeVip;
    s.endpoint.vip = Some("10.0.1.250".parse().unwrap());
    assert!(errors(&s).contains(&Field::EndpointMode), "VIP rejected on cloud VPC");
    assert!(compat::endpoint(true, EndpointMode::KeepalivedHaproxy).is_some());
    assert!(compat::endpoint(false, EndpointMode::KubeVip).is_none(), "VIP fine on baremetal");

    s.endpoint = CpEndpoint { mode: EndpointMode::Dns, vip: None, interface: None, dns_name: None, port: 6443 };
    assert!(errors(&s).contains(&Field::EndpointMode), "DNS mode needs a name");
    s.endpoint.dns_name = Some("api.example.com".into());
    assert!(errors(&s).is_empty(), "{:?}", validate(&s, &CAPS).issues);
    assert!(validate(&s, &CAPS).issues.iter().all(|i| i.field != Field::Interface), "no NIC warning without VIP");

    // Proxmox has no managed LB, so VIP stays allowed there.
    s.target = Target::Cloud { provider: CloudProvider::Proxmox, provisioning: CloudProvider::Proxmox.tofu_defaults().provisioning() };
    assert!(compat::endpoint(s.has_cloud_lb(), EndpointMode::KubeVip).is_none());
}

#[test]
fn dns_endpoint_is_ha_ready() {
    let mut s = Preset::Homelab.build();
    s.topology.ha = false;
    s.topology.control_planes = 1;
    s.endpoint.mode = EndpointMode::Dns;
    s.endpoint.vip = None;
    s.endpoint.dns_name = None;
    assert!(matches!(ha::plan_upgrade(&s, &[], 3), Err(ha::HaError::NoStableEndpoint)));
}
