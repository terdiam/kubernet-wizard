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
