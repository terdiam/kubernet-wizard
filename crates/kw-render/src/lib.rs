//! Renders a deployable project (OpenTofu + Ansible + GitOps) from a spec.
//!
//! Files ending in `.tmpl` are rendered with minijinja; everything else is
//! copied verbatim (zero-copy from the embedded bundle).

pub mod catalog;
pub mod tuning;

use std::borrow::Cow;
use std::fs;
use std::io;
use std::net::IpAddr;
use std::path::Path;

use kw_core::{ClusterSpec, EndpointMode, GitOpsTool, Node, NodeRole, Provisioning, Target};
use kw_distro::{Distro, DistroVars};
use minijinja::{Environment, UndefinedBehavior, Value};
use rust_embed::RustEmbed;
use serde::Serialize;
use smallvec::SmallVec;

use crate::catalog::Chart;

#[derive(RustEmbed)]
#[folder = "../../templates/"]
struct Templates;

const TEMPLATE_EXT: &str = ".tmpl";

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("distro {0} is not available yet")]
    DistroNotReady(&'static str),
    #[error("template {path}: {source}")]
    Template { path: String, source: minijinja::Error },
    #[error("template {0} is not valid UTF-8")]
    Utf8(String),
    #[error("cluster.yaml: {0}")]
    Yaml(String),
    #[error("write {path}: {source}")]
    Io { path: String, source: io::Error },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedFile {
    pub path: String,
    pub contents: Cow<'static, [u8]>,
}

impl RenderedFile {
    /// Text view for previews.
    #[inline]
    pub fn text(&self) -> Cow<'_, str> {
        String::from_utf8_lossy(&self.contents)
    }
}

// ---------------------------------------------------------------- context

#[derive(Serialize)]
struct TofuCtx<'a> {
    region: &'a str,
    cp_size: &'a str,
    worker_size: &'a str,
    image: &'a str,
    /// Ports forwarded by the cloud API load balancer.
    lb_ports: SmallVec<[u16; 2]>,
    hint: &'static str,
}

#[derive(Serialize)]
struct GitopsAddon {
    name: &'static str,
    repo: &'static str,
    chart: &'static str,
    namespace: &'static str,
    version: &'static str,
    values_obj: serde_json::Value,
}

type Roles = SmallVec<[&'static str; 20]>;

#[derive(Serialize)]
struct Ctx<'a> {
    spec: &'a ClusterSpec,
    distro: DistroVars,
    os_family: &'static str,
    endpoint_host: Option<String>,
    api_port: u16,
    tls_sans: SmallVec<[String; 4]>,
    cluster_dns: Option<IpAddr>,
    cp_nodes: SmallVec<[&'a Node; 5]>,
    workers: Vec<&'a Node>,
    tofu: Option<TofuCtx<'a>>,
    tofu_provider: Option<&'static str>,
    ingress_chart: Chart,
    helm_addons: Vec<Chart>,
    gitops_addons: Vec<GitopsAddon>,
    gitops_path: Cow<'a, str>,
    sysctl: std::collections::BTreeMap<&'static str, &'static str>,
    disable_services: &'static [&'static str],
    kubelet: tuning::Kubelet,
    addon_roles: Roles,
}

/// Which optional roles / dirs are part of this project.
struct Selection {
    roles: Roles,
    tofu_provider: Option<&'static str>,
    gitops_tool: Option<&'static str>,
}

impl Selection {
    fn includes(&self, path: &str) -> bool {
        if let Some(rest) = path.strip_prefix("ansible/roles/") {
            let role = rest.split('/').next().unwrap_or_default();
            return self.roles.contains(&role);
        }
        if let Some(rest) = path.strip_prefix("tofu/") {
            return self.tofu_provider.is_some_and(|p| rest.starts_with(p) && rest[p.len()..].starts_with('/'));
        }
        if let Some(rest) = path.strip_prefix("gitops/") {
            return self.gitops_tool.is_some_and(|t| rest.starts_with(t) && rest[t.len()..].starts_with('/'));
        }
        true
    }
}

#[inline]
const fn gitops_dir(tool: GitOpsTool) -> Option<&'static str> {
    match tool {
        GitOpsTool::None => None,
        GitOpsTool::ArgoCd => Some("argo_cd"),
        GitOpsTool::Flux => Some("flux"),
    }
}

/// Roles that run in the "Cluster add-ons" play, in order.
fn addon_roles(spec: &ClusterSpec, ingress: &Chart, has_helm_addons: bool) -> Roles {
    use kw_core::{Cni, DistroId, ServiceLbKind};
    let mut r = Roles::new();
    let cni_by_role = match spec.distro.id {
        DistroId::Rke2 => false,
        DistroId::K3s => spec.network.cni != Cni::Flannel,
        _ => true,
    };
    if cni_by_role {
        r.push(match spec.network.cni {
            Cni::Cilium => "cni_cilium",
            Cni::Calico | Cni::Canal => "cni_calico",
            Cni::Flannel => "cni_flannel",
        });
    }
    match spec.service_lb.kind {
        ServiceLbKind::Metallb => r.push("service_lb_metallb"),
        ServiceLbKind::KubeVip => r.push("service_lb_kubevip"),
        ServiceLbKind::CloudCcm | ServiceLbKind::None => {}
    }
    if !ingress.chart.is_empty() || ingress.gateway_api {
        r.push("ingress");
    }
    if has_helm_addons {
        r.push("addons_helm");
    }
    match spec.gitops.tool {
        GitOpsTool::ArgoCd => r.push("gitops_argocd"),
        GitOpsTool::Flux => r.push("gitops_flux"),
        GitOpsTool::None => {}
    }
    r
}

fn build_ctx(spec: &ClusterSpec, distro: DistroVars) -> (Ctx<'_>, Selection) {
    let ep = &spec.endpoint;
    let cp_nodes: SmallVec<[&Node; 5]> = spec.nodes_with_role(NodeRole::ControlPlane).collect();
    let workers: Vec<&Node> = spec.nodes_with_role(NodeRole::Worker).collect();

    let endpoint_host = match ep.mode {
        EndpointMode::KubeVip | EndpointMode::KeepalivedHaproxy => {
            ep.vip.map(|v| v.to_string()).or_else(|| ep.dns_name.clone())
        }
        EndpointMode::CloudLb | EndpointMode::Dns => ep.dns_name.clone(),
    };
    // keepalived+haproxy runs on CP nodes next to kube-apiserver (6443).
    let api_port = if ep.mode == EndpointMode::KeepalivedHaproxy { 8443 } else { ep.port };

    let mut tls_sans: SmallVec<[String; 4]> = SmallVec::new();
    tls_sans.extend(ep.vip.map(|v| v.to_string()));
    tls_sans.extend(ep.dns_name.iter().cloned());

    let (tofu, tofu_provider) = match &spec.target {
        Target::Cloud { provider, provisioning: Provisioning::Tofu { region, cp_size, worker_size, image } } => (
            Some(TofuCtx {
                region,
                cp_size,
                worker_size,
                image,
                lb_ports: lb_ports(spec),
                hint: provider.tofu_hint(),
            }),
            Some(provider.id()),
        ),
        _ => (None, None),
    };

    let id = spec.distro.id;
    let ingress_chart = catalog::ingress(id, spec.ingress);
    let charts = spec.addons.iter().filter_map(|&a| catalog::addon(id, a));
    let gitops_on = spec.gitops.tool != GitOpsTool::None;
    // Manifest-based add-ons always go through Ansible; Helm ones go to GitOps when enabled.
    let (helm_addons, gitops_addons): (Vec<Chart>, Vec<GitopsAddon>) = if gitops_on {
        let (manifests, helms): (Vec<Chart>, Vec<Chart>) = charts.partition(|c| !c.manifest.is_empty());
        let git = helms
            .into_iter()
            .map(|c| GitopsAddon {
                name: c.name,
                repo: c.repo,
                chart: c.chart,
                namespace: c.namespace,
                version: c.version,
                values_obj: catalog::nest_values(c.values),
            })
            .collect();
        (manifests, git)
    } else {
        (charts.collect(), Vec::new())
    };

    let gitops_path = if spec.gitops.path.trim().is_empty() {
        Cow::Owned(format!("clusters/{}", spec.name))
    } else {
        Cow::Borrowed(spec.gitops.path.trim_matches('/'))
    };

    let roles_addon = addon_roles(spec, &ingress_chart, !helm_addons.is_empty());
    let mut roles: Roles = Roles::new();
    roles.extend_from_slice(&["prereq", "tuning", "helm_bootstrap", "kubeconfig", distro.role]);
    if !distro.bundled_runtime {
        roles.push("containerd");
    }
    match ep.mode {
        EndpointMode::KubeVip => roles.push("cp_endpoint_kubevip"),
        EndpointMode::KeepalivedHaproxy => roles.push("cp_endpoint_keepalived"),
        EndpointMode::Dns => roles.push("cp_endpoint_dns"),
        EndpointMode::CloudLb => {}
    }
    roles.extend_from_slice(&roles_addon);

    let sel = Selection { roles, tofu_provider, gitops_tool: gitops_dir(spec.gitops.tool) };
    let ctx = Ctx {
        spec,
        distro,
        os_family: spec.topology.os.family(),
        endpoint_host,
        api_port,
        tls_sans,
        cluster_dns: spec.network.cluster_dns(),
        cp_nodes,
        workers,
        tofu,
        tofu_provider,
        ingress_chart,
        helm_addons,
        gitops_addons,
        gitops_path,
        sysctl: tuning::sysctl(spec.tuning.profile),
        disable_services: tuning::DISABLE_SERVICES,
        kubelet: tuning::kubelet(&spec.tuning),
        addon_roles: roles_addon,
    };
    (ctx, sel)
}

// ---------------------------------------------------------------- render

/// Renders the whole project into memory (used for UI preview and writing).
pub fn render<D: Distro>(spec: &ClusterSpec, distro: &D) -> Result<Vec<RenderedFile>, RenderError> {
    if !distro.is_ready() {
        return Err(RenderError::DistroNotReady(distro.label()));
    }
    let (ctx, sel) = build_ctx(spec, distro.vars());
    let value = Value::from_serialize(&ctx);
    let gitops_path = ctx.gitops_path.as_ref();

    let mut env = Environment::new();
    env.set_undefined_behavior(UndefinedBehavior::Strict);
    env.set_keep_trailing_newline(true);

    let mut files = Vec::with_capacity(64);
    files.push(RenderedFile {
        path: "cluster.yaml".to_owned(),
        contents: Cow::Owned(spec.to_yaml().map_err(|e| RenderError::Yaml(e.to_string()))?.into_bytes()),
    });

    for path in Templates::iter() {
        if !sel.includes(&path) {
            continue;
        }
        let Some(file) = Templates::get(&path) else { continue };
        let out_path = output_path(&path, sel.gitops_tool, gitops_path);
        let contents = if path.ends_with(TEMPLATE_EXT) {
            let src = std::str::from_utf8(&file.data).map_err(|_| RenderError::Utf8(path.to_string()))?;
            let text = env
                .render_str(src, &value)
                .map_err(|source| RenderError::Template { path: path.to_string(), source })?;
            Cow::Owned(text.into_bytes())
        } else {
            file.data
        };
        files.push(RenderedFile { path: out_path, contents });
    }
    files.sort_unstable_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

/// Strips `.tmpl` and maps `gitops/<tool>/x` → `gitops/<gitops_path>/x`.
fn output_path(path: &str, gitops_tool: Option<&str>, gitops_path: &str) -> String {
    let path = path.strip_suffix(TEMPLATE_EXT).unwrap_or(path);
    if let Some(rest) = gitops_tool.and_then(|t| path.strip_prefix("gitops/")?.strip_prefix(t)) {
        let mut out = String::with_capacity(8 + gitops_path.len() + rest.len());
        out.push_str("gitops/");
        out.push_str(gitops_path);
        out.push_str(rest);
        return out;
    }
    path.to_owned()
}

/// Writes rendered files under `out`. Existing files are overwritten;
/// `ansible/inventory.yml` is kept if OpenTofu already produced one.
pub fn write_all(files: &[RenderedFile], out: &Path, keep_inventory: bool) -> Result<usize, RenderError> {
    let io_err = |p: &Path| {
        let path = p.display().to_string();
        move |source| RenderError::Io { path, source }
    };
    let mut written = 0;
    for f in files {
        let target = out.join(&f.path);
        if keep_inventory && f.path == "ansible/inventory.yml" && target.exists() {
            continue;
        }
        if let Some(dir) = target.parent() {
            fs::create_dir_all(dir).map_err(io_err(dir))?;
        }
        fs::write(&target, &f.contents).map_err(io_err(&target))?;
        written += 1;
    }
    Ok(written)
}

/// Renders with the spec's own distro from the registry.
#[inline]
pub fn render_spec(spec: &ClusterSpec) -> Result<Vec<RenderedFile>, RenderError> {
    render(spec, &kw_distro::get(spec.distro.id))
}

/// API port plus the RKE2 supervisor port (servers join through it).
fn lb_ports(spec: &ClusterSpec) -> SmallVec<[u16; 2]> {
    let mut ports = SmallVec::new();
    ports.push(spec.endpoint.port);
    if spec.distro.id == kw_core::DistroId::Rke2 {
        ports.push(9345);
    }
    ports
}

/// Whether an OpenTofu template exists for a provider.
#[inline]
pub fn has_tofu_template(provider: kw_core::CloudProvider) -> bool {
    let mut path = String::with_capacity(32);
    path.push_str("tofu/");
    path.push_str(provider.id());
    path.push_str("/main.tf.tmpl");
    Templates::get(&path).is_some()
}
