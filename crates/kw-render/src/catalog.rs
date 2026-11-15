//! Static Helm chart / manifest catalog for ingress controllers and add-ons.

use kw_core::{Addon, DistroId, Ingress};
use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct Chart {
    pub name: &'static str,
    /// Helm repo URL; empty for OCI charts (`chart` holds the oci:// ref).
    pub repo: &'static str,
    pub chart: &'static str,
    pub namespace: &'static str,
    /// Empty = latest. Pin via catalog updates.
    pub version: &'static str,
    /// `--set` style values.
    pub values: &'static [&'static str],
    /// Plain manifest URL (kubectl apply) instead of Helm.
    pub manifest: &'static str,
    /// Requires Gateway API CRDs.
    pub gateway_api: bool,
}

const fn helm(name: &'static str, repo: &'static str, chart: &'static str, namespace: &'static str, values: &'static [&'static str]) -> Chart {
    Chart { name, repo, chart, namespace, version: "", values, manifest: "", gateway_api: false }
}

const EMPTY: Chart = helm("", "", "", "", &[]);

/// Ingress controller chart. `EMPTY.chart == ""` means nothing to install.
pub fn ingress(distro: DistroId, ingress: Ingress) -> Chart {
    match ingress {
        // k3s ships Traefik itself.
        Ingress::Traefik if distro == DistroId::K3s => EMPTY,
        Ingress::Traefik => helm("traefik", "https://traefik.github.io/charts", "traefik", "traefik", &[]),
        Ingress::HaproxyIngress => helm(
            "haproxy-ingress",
            "https://haproxytech.github.io/helm-charts",
            "kubernetes-ingress",
            "haproxy-ingress",
            &[],
        ),
        Ingress::EnvoyGateway => helm(
            "envoy-gateway",
            "",
            "oci://docker.io/envoyproxy/gateway-helm",
            "envoy-gateway-system",
            &[],
        ),
        Ingress::CiliumGateway => Chart { gateway_api: true, ..EMPTY },
        Ingress::NginxGatewayFabric => Chart {
            gateway_api: true,
            ..helm("nginx-gateway", "", "oci://ghcr.io/nginx/charts/nginx-gateway-fabric", "nginx-gateway", &[])
        },
        Ingress::Contour => helm("contour", "https://projectcontour.github.io/helm-charts/", "contour", "projectcontour", &[]),
        Ingress::None => EMPTY,
    }
}

/// Add-on chart. `None` = provided by the distro itself.
pub fn addon(distro: DistroId, addon: Addon) -> Option<Chart> {
    Some(match addon {
        Addon::LocalPath if distro == DistroId::K3s => return None,
        Addon::LocalPath => Chart {
            manifest: "https://raw.githubusercontent.com/rancher/local-path-provisioner/master/deploy/local-path-storage.yaml",
            ..helm("local-path", "", "", "local-path-storage", &[])
        },
        Addon::CertManager => helm("cert-manager", "https://charts.jetstack.io", "cert-manager", "cert-manager", &["crds.enabled=true"]),
        Addon::MetricsServer => helm(
            "metrics-server",
            "https://kubernetes-sigs.github.io/metrics-server/",
            "metrics-server",
            "kube-system",
            &["args[0]=--kubelet-insecure-tls"],
        ),
        Addon::Longhorn => helm("longhorn", "https://charts.longhorn.io", "longhorn", "longhorn-system", &["persistence.defaultClassReplicaCount=2"]),
        Addon::Openebs => helm("openebs", "https://openebs.github.io/openebs", "openebs", "openebs", &["engines.replicated.mayastor.enabled=false"]),
        Addon::RookCeph => helm("rook-ceph", "https://charts.rook.io/release", "rook-ceph", "rook-ceph", &[]),
        Addon::VictoriaMetrics => helm(
            "victoria-metrics",
            "https://victoriametrics.github.io/helm-charts/",
            "victoria-metrics-k8s-stack",
            "monitoring",
            &[],
        ),
        Addon::KubePrometheusStack => helm(
            "kube-prometheus-stack",
            "https://prometheus-community.github.io/helm-charts",
            "kube-prometheus-stack",
            "monitoring",
            &[],
        ),
        Addon::Loki => helm(
            "loki",
            "https://grafana.github.io/helm-charts",
            "loki",
            "logging",
            &["deploymentMode=SingleBinary", "singleBinary.replicas=1"],
        ),
        Addon::Velero => helm("velero", "https://vmware-tanzu.github.io/helm-charts", "velero", "velero", &[]),
        Addon::Kyverno => helm("kyverno", "https://kyverno.github.io/kyverno/", "kyverno", "kyverno", &[]),
        Addon::ExternalSecrets => helm("external-secrets", "https://charts.external-secrets.io", "external-secrets", "external-secrets", &[]),
        Addon::SealedSecrets => helm("sealed-secrets", "https://bitnami-labs.github.io/sealed-secrets", "sealed-secrets", "kube-system", &[]),
        Addon::NodeLocalDns => helm("node-local-dns", "https://charts.deliveryhero.io/", "node-local-dns", "kube-system", &[]),
        Addon::Descheduler => helm("descheduler", "https://kubernetes-sigs.github.io/descheduler/", "descheduler", "kube-system", &[]),
        Addon::ClusterAutoscaler => helm("cluster-autoscaler", "https://kubernetes.github.io/autoscaler", "cluster-autoscaler", "kube-system", &[]),
        Addon::Vcluster => helm("vcluster", "https://charts.loft.sh", "vcluster", "vcluster", &[]),
    })
}

/// Converts `a.b=v` pairs into a nested JSON object for GitOps values.
pub fn nest_values(values: &[&str]) -> serde_json::Value {
    use serde_json::{Map, Value};
    let mut root = Map::with_capacity(values.len());
    for (key, raw) in values.iter().filter_map(|kv| kv.split_once('=')) {
        let leaf = match raw {
            "true" => Value::Bool(true),
            "false" => Value::Bool(false),
            _ => raw.parse::<i64>().map_or_else(|_| Value::String(raw.to_owned()), Value::from),
        };
        let mut parts = key.split('.').peekable();
        let mut cur = &mut root;
        while let Some(part) = parts.next() {
            if parts.peek().is_none() {
                cur.insert(part.to_owned(), leaf);
                break;
            }
            let entry = cur.entry(part.to_owned()).or_insert_with(|| Value::Object(Map::new()));
            if !entry.is_object() {
                *entry = Value::Object(Map::new());
            }
            match entry {
                Value::Object(m) => cur = m,
                _ => break,
            }
        }
    }
    Value::Object(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nests_dotted_keys() {
        let v = nest_values(&["a.b=true", "a.c=2", "d=x"]);
        assert_eq!(v.to_string(), r#"{"a":{"b":true,"c":2},"d":"x"}"#);
    }

    #[test]
    fn k3s_bundles_traefik_and_local_path() {
        assert_eq!(ingress(DistroId::K3s, Ingress::Traefik).chart, "");
        assert!(addon(DistroId::K3s, Addon::LocalPath).is_none());
        assert_ne!(ingress(DistroId::Kubeadm, Ingress::Traefik).chart, "");
    }
}
