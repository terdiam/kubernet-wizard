#![allow(clippy::unwrap_used, clippy::case_sensitive_file_extension_comparisons)]
use kw_core::presets::Preset;
use kw_render::render_spec;

fn paths(p: Preset) -> Vec<String> {
    render_spec(&p.build()).unwrap().into_iter().map(|f| f.path).collect()
}

fn file(p: Preset, path: &str) -> String {
    render_spec(&p.build())
        .unwrap()
        .into_iter()
        .find(|f| f.path == path)
        .unwrap_or_else(|| panic!("{path} missing"))
        .text()
        .into_owned()
}

#[test]
fn every_preset_renders() {
    for p in Preset::ALL {
        let files = render_spec(&p.build()).unwrap();
        assert!(files.iter().any(|f| f.path == "ansible/site.yml"), "{p:?}");
        assert!(files.iter().all(|f| !f.path.ends_with(".tmpl")));
    }
}

#[test]
fn only_selected_roles_rendered() {
    let dev = paths(Preset::DevLocal);
    assert!(dev.iter().any(|p| p.starts_with("ansible/roles/distro_k3s/")));
    assert!(!dev.iter().any(|p| p.starts_with("ansible/roles/distro_kubeadm/")));
    assert!(!dev.iter().any(|p| p.starts_with("ansible/roles/containerd/")));
    assert!(!dev.iter().any(|p| p.starts_with("ansible/roles/cni_")), "k3s flannel is built in");
    assert!(!dev.iter().any(|p| p.starts_with("tofu/")));

    let prod = paths(Preset::ProductionOnPrem);
    assert!(prod.iter().any(|p| p.starts_with("ansible/roles/containerd/")));
    assert!(prod.iter().any(|p| p.starts_with("ansible/roles/cni_cilium/")));
    assert!(prod.iter().any(|p| p == "gitops/clusters/prod/addons.yaml"));

    let cloud = paths(Preset::CloudHetzner);
    assert!(cloud.iter().any(|p| p == "tofu/hetzner/main.tf"));
    assert!(!cloud.iter().any(|p| p.starts_with("ansible/roles/cp_endpoint_")));
}

#[test]
fn snapshot_group_vars_homelab() {
    insta::assert_snapshot!(file(Preset::Homelab, "ansible/group_vars/all.yml"));
}

#[test]
fn snapshot_site_prod() {
    insta::assert_snapshot!(file(Preset::ProductionOnPrem, "ansible/site.yml"));
}

#[test]
fn snapshot_inventory_homelab() {
    insta::assert_snapshot!(file(Preset::Homelab, "ansible/inventory.yml"));
}

#[test]
fn snapshot_gitops_prod() {
    insta::assert_snapshot!(file(Preset::ProductionOnPrem, "gitops/clusters/prod/addons.yaml"));
}

#[test]
fn snapshot_tofu_hetzner() {
    insta::assert_snapshot!(file(Preset::CloudHetzner, "tofu/hetzner/main.tf"));
}

fn cloud_spec(provider: kw_core::CloudProvider, cloud_lb: bool) -> kw_core::ClusterSpec {
    use kw_core::{EndpointMode, Target};
    let mut s = Preset::CloudHetzner.build();
    s.target = Target::Cloud { provider, provisioning: provider.tofu_defaults().provisioning() };
    if !cloud_lb && provider.has_cloud_lb() {
        // Cloud VPCs cannot float a VIP: DNS-name endpoint instead.
        s.endpoint.mode = EndpointMode::Dns;
        s.endpoint.dns_name = Some("api.example.com".into());
    } else if !cloud_lb {
        s.endpoint.mode = EndpointMode::KubeVip;
        s.endpoint.vip = Some("10.0.1.250".parse().unwrap());
    }
    s
}

#[test]
fn dns_endpoint_pins_hosts_without_vip() {
    use kw_core::EndpointMode;
    let mut s = Preset::Homelab.build();
    s.endpoint.mode = EndpointMode::Dns;
    s.endpoint.vip = None;
    s.endpoint.dns_name = Some("api.lab.example".into());
    let files = render_spec(&s).unwrap();
    let get = |p: &str| files.iter().find(|f| f.path == p).unwrap_or_else(|| panic!("{p} missing")).text().into_owned();
    assert!(get("ansible/site.yml").contains("- cp_endpoint_dns"));
    assert!(!get("ansible/site.yml").contains("cp_endpoint_kubevip"));
    assert!(get("ansible/group_vars/all.yml").contains("kw_endpoint_host: \"api.lab.example\""));
    assert!(files.iter().any(|f| f.path == "ansible/roles/cp_endpoint_dns/tasks/main.yml"));
    assert!(!files.iter().any(|f| f.path.starts_with("ansible/roles/cp_endpoint_kubevip/")));
}

#[test]
fn every_provider_renders_tofu() {
    for &p in kw_core::CloudProvider::ALL {
        assert!(kw_render::has_tofu_template(p), "{p:?}");
        for lb in [true, false] {
            if lb && !p.has_cloud_lb() {
                continue;
            }
            let files = render_spec(&cloud_spec(p, lb)).unwrap();
            let main = format!("tofu/{}/main.tf", p.id());
            let tf = files.iter().find(|f| f.path == main).unwrap_or_else(|| panic!("{main} missing"));
            let text = tf.text();
            assert!(!text.contains("{{") && !text.contains("{%"), "{p:?}: unrendered template syntax");
            assert!(text.contains("local_file\" \"inventory\""), "{p:?}");
            assert_eq!(text.contains("kw_endpoint_host"), lb, "{p:?} lb={lb}");
            assert!(files.iter().filter(|f| f.path.starts_with("tofu/")).all(|f| f.path.starts_with(&format!("tofu/{}/", p.id()))));
        }
    }
}

#[test]
fn rke2_lb_forwards_supervisor_port() {
    let mut s = cloud_spec(kw_core::CloudProvider::Aws, true);
    s.distro.id = kw_core::DistroId::Rke2;
    let tf = render_spec(&s).unwrap().into_iter().find(|f| f.path == "tofu/aws/main.tf").unwrap();
    assert!(tf.text().contains("lb_ports = [6443, 9345]"));
}
