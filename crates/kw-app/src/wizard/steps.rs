//! Per-step content.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::{Input, Textarea};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme, Disableable, Icon, IconName, Selectable, Sizable, WindowExt, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use gpui_kit::base::StyledExt as _;
use kw_core::{
    Addon, Advertisement, CloudProvider, Cni, DistroId, EndpointMode, Field, GitOpsTool, Ingress,
    KubeProxyMode, NodeOs, Provisioning, ServiceLbKind, Step, Target, TuningProfile, compat,
};
use kw_distro::{Distro, REGISTRY};

use super::deploy::{Phase, StepState};
use super::{In, Wizard};
use crate::docs::DocPage;
use crate::help::Topic;
use crate::i18n::{self, K};

const ADDON_GROUPS: [(&str, &[Addon]); 4] = [
    ("Storage", &[Addon::LocalPath, Addon::Longhorn, Addon::Openebs, Addon::RookCeph]),
    ("Observability", &[Addon::MetricsServer, Addon::VictoriaMetrics, Addon::KubePrometheusStack, Addon::Loki]),
    ("Security", &[Addon::CertManager, Addon::Kyverno, Addon::ExternalSecrets, Addon::SealedSecrets]),
    ("Operations", &[Addon::Velero, Addon::NodeLocalDns, Addon::Descheduler, Addon::ClusterAutoscaler, Addon::Vcluster]),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum TargetKind {
    Baremetal,
    Cloud,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ProvKind {
    Tofu,
    Existing,
}

#[inline]
fn ep_needs_dns(spec: &kw_core::ClusterSpec) -> bool {
    spec.endpoint.mode == EndpointMode::CloudLb
        && matches!(spec.target, Target::Cloud { provisioning: Provisioning::ExistingVms, .. })
}

#[inline]
fn default_tofu(provider: CloudProvider) -> Provisioning {
    provider.tofu_defaults().provisioning()
}

impl Wizard {
    #[inline]
    fn input(&self, key: In) -> Input {
        Input::new(&self.inputs[key as usize]).small()
    }

    pub(super) fn render_step(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        match self.step {
            Step::Project => self.step_project(cx).into_any_element(),
            Step::Target => self.step_target(cx).into_any_element(),
            Step::Distro => self.step_distro(cx).into_any_element(),
            Step::Topology => self.step_topology(cx).into_any_element(),
            Step::Endpoint => self.step_endpoint(cx).into_any_element(),
            Step::Network => self.step_network(cx).into_any_element(),
            Step::Ingress => self.step_ingress(cx).into_any_element(),
            Step::Features => self.step_features(cx).into_any_element(),
            Step::GitOps => self.step_gitops(cx).into_any_element(),
            Step::Tuning => self.step_tuning(cx).into_any_element(),
            Step::Review => self.step_review(cx).into_any_element(),
            Step::Deploy => self.step_deploy(window, cx).into_any_element(),
        }
    }

    fn step_project(&self, cx: &mut Context<Self>) -> Div {
        let muted = cx.theme().muted_foreground;
        v_flex()
            .gap_4()
            .child(self.field("f-name", "Cluster name", Some(Field::Name), Some(DocPage::GettingStarted), self.input(In::Name), cx))
            .child(
                self.field(
                    "f-dir",
                    "Project folder",
                    None,
                    None,
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(div().flex_1().text_sm().child(self.project_dir.display().to_string()))
                        .child(
                            Button::new("pick-dir")
                                .small()
                                .outline()
                                .icon(IconName::FolderOpen)
                                .label("…")
                                .on_click(cx.listener(|this, _, _, cx| this.pick_project_parent(cx))),
                        ),
                    cx,
                ),
            )
            .child(div().text_xs().text_color(muted).child(
                "cluster.yaml, OpenTofu, Ansible and GitOps files are written here when you press Generate or Deploy.",
            ))
    }

    fn pick_project_parent(&mut self, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Choose parent folder".into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await
                && let Some(dir) = paths.into_iter().next()
            {
                let _ = this.update(cx, |w, cx| {
                    w.project_dir = dir.join(&w.spec.name);
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn step_target(&self, cx: &mut Context<Self>) -> Div {
        let kind = if self.spec.is_cloud() { TargetKind::Cloud } else { TargetKind::Baremetal };
        let mut col = v_flex().gap_4().child(self.field(
            "f-target",
            "Where will the cluster run?",
            Some(Field::Target),
            None,
            self.chips(
                "target",
                &[TargetKind::Baremetal, TargetKind::Cloud],
                kind,
                |k| if k == TargetKind::Baremetal { "Baremetal / existing servers" } else { "Cloud" },
                |_| None,
                None,
                None,
                |s, k| {
                    s.target = match k {
                        TargetKind::Baremetal => Target::Baremetal,
                        TargetKind::Cloud => Target::Cloud { provider: CloudProvider::Hetzner, provisioning: default_tofu(CloudProvider::Hetzner) },
                    };
                },
                cx,
            ),
            cx,
        ));
        if let Target::Cloud { provider, provisioning } = &self.spec.target {
            let provider = *provider;
            let prov = if matches!(provisioning, Provisioning::Tofu { .. }) { ProvKind::Tofu } else { ProvKind::Existing };
            col = col
                .child(self.field(
                    "f-provider",
                    "Provider",
                    None,
                    None,
                    self.chips(
                        "provider",
                        CloudProvider::ALL,
                        provider,
                        CloudProvider::label,
                        |_| None,
                        None,
                        None,
                        |s, p| {
                            if let Target::Cloud { provider, provisioning } = &mut s.target {
                                *provider = p;
                                if matches!(provisioning, Provisioning::Tofu { .. }) {
                                    *provisioning = default_tofu(p);
                                }
                            }
                        },
                        cx,
                    ),
                    cx,
                ))
                .child(self.field(
                    "f-prov",
                    "Virtual machines",
                    Some(Field::Provisioning),
                    None,
                    self.chips(
                        "prov",
                        &[ProvKind::Tofu, ProvKind::Existing],
                        prov,
                        |p| if p == ProvKind::Tofu { "Create with OpenTofu" } else { "Use existing VMs" },
                        move |p| {
                            (p == ProvKind::Tofu && !kw_render::has_tofu_template(provider))
                                .then_some("OpenTofu template for this provider is coming soon — use existing VMs for now")
                        },
                        None,
                        None,
                        |s, p| {
                            if let Target::Cloud { provider, provisioning } = &mut s.target {
                                *provisioning = if p == ProvKind::Tofu { default_tofu(*provider) } else { Provisioning::ExistingVms };
                            }
                        },
                        cx,
                    ),
                    cx,
                ));
            if prov == ProvKind::Tofu {
                col = col.child(
                    h_flex()
                        .gap_3()
                        .child(div().flex_1().child(self.field("f-region", "Region", None, None, self.input(In::Region), cx)))
                        .child(div().flex_1().child(self.field("f-cps", "CP size", None, None, self.input(In::CpSize), cx)))
                        .child(div().flex_1().child(self.field("f-ws", "Worker size", None, None, self.input(In::WorkerSize), cx)))
                        .child(div().flex_1().child(self.field("f-img", "Image", None, None, self.input(In::Image), cx))),
                )
                .child(div().text_xs().text_color(cx.theme().muted_foreground).child(provider.tofu_hint()));
            }
        }
        col
    }

    fn step_distro(&self, cx: &mut Context<Self>) -> Div {
        let theme = cx.theme();
        let (border, muted, primary) = (theme.border, theme.muted_foreground, theme.primary);
        let current = self.spec.distro.id;
        let caps = kw_distro::get(current).capabilities();
        let cards = div().grid().grid_cols(3).gap_3().children(REGISTRY.iter().enumerate().map(|(i, d)| {
            let id = d.id();
            let ready = d.is_ready();
            let selected = id == current;
            v_flex()
                .id(("distro-card", i))
                .p_3()
                .gap_1()
                .rounded_md()
                .border_1()
                .border_color(if selected { primary } else { border })
                .when(!ready, |c| c.opacity(0.55))
                .child(
                    h_flex()
                        .justify_between()
                        .child(div().font_semibold().child(id.label()))
                        .child(div().text_xs().text_color(muted).child(kw_distro::phase_label(d.capabilities().phase))),
                )
                .child(div().text_xs().text_color(muted).child(d.description()))
                .child(
                    Button::new(("distro-pick", i))
                        .small()
                        .when(selected, |b| b.primary())
                        .when(!selected, |b| b.outline())
                        .label(if selected { "Selected" } else { "Select" })
                        .selected(selected)
                        .disabled(!ready)
                        .when(!ready, |b| b.tooltip("Planned for a later release"))
                        .on_hover(cx.listener(move |this, h: &bool, _, cx| {
                            if *h {
                                this.topic = Some(Topic::Distro(id));
                                cx.notify();
                            }
                        }))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.edit(
                                |s| {
                                    let d = kw_distro::get(id);
                                    d.default_version().clone_into(&mut s.distro.version);
                                    s.distro.id = id;
                                    let caps = d.capabilities();
                                    if !caps.supports_cni(s.network.cni) {
                                        s.network.cni = caps.cnis.first().copied().unwrap_or(Cni::Cilium);
                                    }
                                },
                                cx,
                            );
                        })),
                )
        }));
        v_flex()
            .gap_4()
            .child(cards)
            .child(self.field(
                "f-version",
                "Version",
                Some(Field::Version),
                None,
                h_flex().gap_2().children(caps.versions.iter().enumerate().map(|(i, &v)| {
                    let selected = self.spec.distro.version == v;
                    Button::new(("ver", i))
                        .small()
                        .label(v)
                        .selected(selected)
                        .when(selected, |b| b.primary())
                        .when(!selected, |b| b.outline())
                        .when(i == 0, |b| b.tooltip("Latest tested"))
                        .on_click(cx.listener(move |this, _, _, cx| this.edit(|s| v.clone_into(&mut s.distro.version), cx)))
                })),
                cx,
            ))
    }

    fn topology_diagram(&self, cx: &mut Context<Self>) -> Div {
        let theme = cx.theme();
        let (border, primary, muted, card) = (theme.border, theme.primary, theme.muted_foreground, theme.secondary);
        let t = &self.spec.topology;
        let ep = match self.spec.endpoint.mode {
            EndpointMode::CloudLb => "Cloud LB".to_owned(),
            _ => self.spec.endpoint.vip.map_or_else(|| "VIP ?".to_owned(), |v| format!("VIP {v}")),
        };
        let boxes = |prefix: &'static str, n: u8, color: Hsla| {
            h_flex().gap_2().flex_wrap().justify_center().children((0..n.min(12)).map(move |i| {
                div()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .border_1()
                    .border_color(color)
                    .text_xs()
                    .child(format!("{prefix}-{}", i + 1))
            }))
        };
        v_flex()
            .p_3()
            .gap_2()
            .items_center()
            .rounded_md()
            .bg(card)
            .child(div().text_xs().text_color(muted).child("kubectl / users"))
            .child(div().px_3().py_1().rounded_md().border_1().border_color(primary).text_sm().child(ep))
            .child(div().text_xs().text_color(muted).child("↓ API :6443"))
            .child(boxes("cp", t.control_planes, primary))
            .when(t.workers > 0, |d| d.child(div().text_xs().text_color(muted).child("↓")).child(boxes("worker", t.workers, border)))
            .when(t.workers > 12, |d| d.child(div().text_xs().text_color(muted).child(format!("… {} workers", t.workers))))
    }

    fn step_topology(&self, cx: &mut Context<Self>) -> Div {
        let t = &self.spec.topology;
        let caps = kw_distro::caps_for(&self.spec);
        let needs_inventory = self.spec.tofu_provider().is_none();
        let workers = t.workers;
        let muted = cx.theme().muted_foreground;
        v_flex()
            .gap_4()
            .child(self.field(
                "f-ha",
                "High availability",
                Some(Field::ControlPlanes),
                Some(DocPage::HighAvailability),
                v_flex()
                    .gap_2()
                    .child(
                        Switch::new("ha")
                            .label("HA control plane (3 or 5 nodes)")
                            .checked(t.ha)
                            .on_change(cx.listener(|this, on: &bool, _, cx| {
                                let on = *on;
                                this.edit(
                                    |s| {
                                        s.topology.ha = on;
                                        s.topology.control_planes = if on { 3 } else { 1 };
                                    },
                                    cx,
                                );
                            })),
                    )
                    .when(t.ha, |d| {
                        d.child(self.chips(
                            "cps",
                            &[3u8, 5],
                            t.control_planes,
                            |n| if n == 3 { "3 control planes" } else { "5 control planes" },
                            |_| None,
                            Some(3),
                            None,
                            |s, n| s.topology.control_planes = n,
                            cx,
                        ))
                    })
                    .when(!t.ha, |d| {
                        d.child(div().text_xs().text_color(muted).child(
                            "Single control plane, built HA-ready: you can upgrade to 3 control planes later without rebuilding.",
                        ))
                    }),
                cx,
            ))
            .child(self.field(
                "f-workers",
                "Workers",
                Some(Field::Workers),
                None,
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        Button::new("w-minus")
                            .small()
                            .outline()
                            .icon(IconName::Minus)
                            .disabled(workers == 0)
                            .on_click(cx.listener(|this, _, _, cx| this.edit(|s| s.topology.workers = s.topology.workers.saturating_sub(1), cx))),
                    )
                    .child(div().w(px(36.)).text_center().font_semibold().child(workers.to_string()))
                    .child(
                        Button::new("w-plus")
                            .small()
                            .outline()
                            .icon(IconName::Plus)
                            .disabled(workers >= 250)
                            .on_click(cx.listener(|this, _, _, cx| this.edit(|s| s.topology.workers = s.topology.workers.saturating_add(1), cx))),
                    ),
                cx,
            ))
            .child(self.field(
                "f-os",
                "Node OS",
                Some(Field::Os),
                None,
                self.chips("os", NodeOs::ALL, t.os, NodeOs::label, |o| compat::os(&caps, o), Some(NodeOs::Ubuntu2404), None, |s, o| s.topology.os = o, cx),
                cx,
            ))
            .when(needs_inventory, |d| {
                d.child(self.field(
                    "f-nodes",
                    "Inventory (one node per line: name ip cp|worker [ssh-user] [ssh-key])",
                    Some(Field::Nodes),
                    Some(DocPage::Troubleshooting),
                    Textarea::new(&self.nodes).h(px(140.)),
                    cx,
                ))
            })
            .child(self.topology_diagram(cx))
    }

    fn step_endpoint(&self, cx: &mut Context<Self>) -> Div {
        let cloud = self.spec.has_cloud_lb();
        // Existing VMs: the user's own LB address must be entered.
        let needs_dns = ep_needs_dns(&self.spec);
        let ep = &self.spec.endpoint;
        let lb = &self.spec.service_lb;
        let adv = Self::advanced(cx);
        let uses_vip = ep.mode != EndpointMode::CloudLb;
        let uses_pool = matches!(lb.kind, ServiceLbKind::Metallb | ServiceLbKind::KubeVip);
        v_flex()
            .gap_4()
            .child(self.field(
                "f-ep",
                "Control-plane endpoint",
                Some(Field::EndpointMode),
                Some(DocPage::EndpointLb),
                self.chips(
                    "ep",
                    &[EndpointMode::KubeVip, EndpointMode::KeepalivedHaproxy, EndpointMode::CloudLb],
                    ep.mode,
                    EndpointMode::label,
                    move |m| compat::endpoint(cloud, m),
                    Some(if cloud { EndpointMode::CloudLb } else { EndpointMode::KubeVip }),
                    Some(Topic::Endpoint),
                    |s, m| s.endpoint.mode = m,
                    cx,
                ),
                cx,
            ))
            .when(uses_vip, |d| {
                d.child(
                    h_flex()
                        .gap_3()
                        .child(div().flex_1().child(self.field("f-vip", "Virtual IP (free IP in node subnet)", Some(Field::Vip), Some(DocPage::EndpointLb), self.input(In::Vip), cx)))
                        .child(div().w(px(160.)).child(self.field("f-iface", "Interface", Some(Field::Interface), None, self.input(In::Iface), cx))),
                )
            })
            .when(adv || needs_dns, |d| {
                let label = if needs_dns { "Load balancer DNS name or IP" } else { "DNS name (optional, added to certificate)" };
                d.child(self.field("f-dns", label, None, None, self.input(In::DnsName), cx))
            })
            .child(self.field(
                "f-slb",
                "Service LoadBalancer",
                Some(Field::ServiceLb),
                Some(DocPage::EndpointLb),
                self.chips(
                    "slb",
                    ServiceLbKind::ALL,
                    lb.kind,
                    ServiceLbKind::label,
                    move |k| compat::service_lb(cloud, k),
                    Some(if cloud { ServiceLbKind::CloudCcm } else { ServiceLbKind::Metallb }),
                    Some(Topic::ServiceLb),
                    |s, k| s.service_lb.kind = k,
                    cx,
                ),
                cx,
            ))
            .when(uses_pool, |d| {
                d.when(lb.kind == ServiceLbKind::Metallb, |d| {
                    d.child(self.field(
                        "f-adv",
                        "Advertisement",
                        None,
                        None,
                        self.chips("adv", Advertisement::ALL, lb.advertisement, Advertisement::label, |_| None, Some(Advertisement::L2), None, |s, a| s.service_lb.advertisement = a, cx),
                        cx,
                    ))
                })
                .child(self.field("f-pool", "Address pool", Some(Field::LbPool), Some(DocPage::EndpointLb), self.input(In::Pool), cx))
            })
    }

    fn step_network(&self, cx: &mut Context<Self>) -> Div {
        let caps = kw_distro::caps_for(&self.spec);
        let net = &self.spec.network;
        let cni = net.cni;
        let adv = Self::advanced(cx);
        let muted = cx.theme().muted_foreground;
        let dns = net.cluster_dns().map_or_else(String::new, |d| d.to_string());
        let recommended = caps.cnis.first().copied();
        v_flex()
            .gap_4()
            .child(self.field(
                "f-cni",
                "CNI",
                Some(Field::Cni),
                Some(DocPage::Network),
                self.chips("cni", Cni::ALL, cni, Cni::label, |c| compat::cni(&caps, c), recommended, Some(Topic::Cni), |s, c| {
                    s.network.cni = c;
                    if c != Cni::Cilium && s.network.kube_proxy == KubeProxyMode::None {
                        s.network.kube_proxy = KubeProxyMode::Iptables;
                    }
                }, cx),
                cx,
            ))
            .child(
                h_flex()
                    .gap_3()
                    .child(div().flex_1().child(self.field("f-pod", "Pod CIDR", Some(Field::PodCidr), Some(DocPage::Network), self.input(In::PodCidr), cx)))
                    .child(div().flex_1().child(self.field("f-svc", "Service CIDR (ClusterIP range)", Some(Field::ServiceCidr), None, self.input(In::SvcCidr), cx)))
                    .child(div().flex_1().child(self.field("f-node", "Node subnet", Some(Field::NodeCidr), None, self.input(In::NodeCidr), cx))),
            )
            .child(div().text_xs().text_color(muted).child(format!("Cluster DNS IP: {dns} (10th address of the Service CIDR)")))
            .when(adv, |d| {
                d.child(self.field(
                    "f-kp",
                    "kube-proxy mode",
                    Some(Field::KubeProxy),
                    Some(DocPage::Network),
                    self.chips("kp", KubeProxyMode::ALL, net.kube_proxy, KubeProxyMode::label, move |m| compat::kube_proxy(&caps, cni, m), None, None, |s, m| s.network.kube_proxy = m, cx),
                    cx,
                ))
                .child(
                    h_flex()
                        .gap_3()
                        .child(div().flex_1().child(self.field("f-domain", "Cluster domain", None, None, self.input(In::Domain), cx)))
                        .child(div().w(px(140.)).child(self.field("f-mtu", "MTU", Some(Field::Mtu), None, self.input(In::Mtu), cx))),
                )
            })
    }

    fn step_ingress(&self, cx: &mut Context<Self>) -> Div {
        let cni = self.spec.network.cni;
        let rec = if self.spec.distro.id == DistroId::K3s { Ingress::Traefik } else { Ingress::EnvoyGateway };
        let muted = cx.theme().muted_foreground;
        v_flex()
            .gap_4()
            .child(self.field(
                "f-ingress",
                "Ingress / Gateway controller",
                Some(Field::Ingress),
                Some(DocPage::IngressGitOps),
                self.chips("ing", Ingress::ALL, self.spec.ingress, Ingress::label, move |i| compat::ingress(cni, i), Some(rec), Some(Topic::Ingress), |s, i| s.ingress = i, cx),
                cx,
            ))
            .child(div().text_xs().text_color(muted).child("ingress-nginx is not offered: the project was retired in March 2026."))
    }

    fn step_features(&self, cx: &mut Context<Self>) -> Div {
        let theme = cx.theme();
        let (card, muted) = (theme.secondary, theme.muted_foreground);
        let cloud = self.spec.is_cloud();
        let mut grid = div().grid().grid_cols(2).gap_3();
        for (gi, (group, addons)) in ADDON_GROUPS.iter().enumerate() {
            let mut col = v_flex().p_3().gap_2().rounded_md().bg(card).child(div().font_semibold().child(*group));
            for (ai, &a) in addons.iter().enumerate() {
                let on = self.spec.has_addon(a);
                let why = (a == Addon::ClusterAutoscaler && !cloud).then_some("Cloud targets only");
                let check = Checkbox::new(("addon", gi * 10 + ai))
                    .label(a.label())
                    .checked(on)
                    .disabled(why.is_some())
                    .on_change(cx.listener(move |this, v: &bool, _, cx| {
                        let v = *v;
                        this.edit(
                            |s| {
                                if v {
                                    if !s.addons.contains(&a) {
                                        s.addons.push(a);
                                    }
                                } else {
                                    s.addons.retain(|x| *x != a);
                                }
                            },
                            cx,
                        );
                    }));
                col = col.child(
                    h_flex().gap_2().child(check).when_some(why, |r, w| r.child(div().text_xs().text_color(muted).child(w))),
                );
            }
            grid = grid.child(col);
        }
        grid
    }

    fn step_gitops(&self, cx: &mut Context<Self>) -> Div {
        let on = self.spec.gitops.tool != GitOpsTool::None;
        v_flex()
            .gap_4()
            .child(self.field(
                "f-gitops",
                "GitOps tool",
                None,
                Some(DocPage::IngressGitOps),
                self.chips("gitops", GitOpsTool::ALL, self.spec.gitops.tool, GitOpsTool::label, |_| None, Some(GitOpsTool::ArgoCd), Some(Topic::GitOps), |s, t| {
                    s.gitops.tool = t;
                    if s.gitops.branch.is_empty() {
                        "main".clone_into(&mut s.gitops.branch);
                    }
                }, cx),
                cx,
            ))
            .when(on, |d| {
                d.child(self.field("f-repo", "Repository URL", Some(Field::GitOpsRepo), None, self.input(In::RepoUrl), cx)).child(
                    h_flex()
                        .gap_3()
                        .child(div().w(px(160.)).child(self.field("f-branch", "Branch", None, None, self.input(In::Branch), cx)))
                        .child(div().flex_1().child(self.field("f-path", "Path in repo", None, None, self.input(In::GitPath), cx))),
                )
            })
    }

    fn step_tuning(&self, cx: &mut Context<Self>) -> Div {
        let t = &self.spec.tuning;
        let k = kw_render::tuning::kubelet(t);
        let muted = cx.theme().muted_foreground;
        v_flex()
            .gap_4()
            .child(self.field(
                "f-tuning",
                "Profile",
                None,
                Some(DocPage::Tuning),
                self.chips("tune", TuningProfile::ALL, t.profile, TuningProfile::label, |_| None, Some(TuningProfile::Minimal), Some(Topic::Tuning), |s, p| s.tuning.profile = p, cx),
                cx,
            ))
            .child(div().w(px(200.)).child(self.field("f-mem", "RAM per node (GiB)", None, Some(DocPage::Tuning), self.input(In::NodeMem), cx)))
            .child(
                v_flex()
                    .gap_1()
                    .text_xs()
                    .text_color(muted)
                    .child(format!("kubelet systemReserved: {}", k.system_reserved))
                    .child(format!("kubelet kubeReserved: {}", k.kube_reserved))
                    .child(format!("evictionHard: {}", k.eviction_hard))
                    .child(format!("image GC: {}% / {}% · maxPods {} · cpuManager {}", k.image_gc_high, k.image_gc_low, k.max_pods, k.cpu_manager_policy))
                    .child(format!("{} sysctl keys · THP madvise · journald capped · idle services disabled", kw_render::tuning::sysctl(t.profile).len())),
            )
    }

    fn step_review(&self, cx: &mut Context<Self>) -> Div {
        let theme = cx.theme();
        let (danger, warning, border, muted, card) = (theme.danger, theme.warning, theme.border, theme.muted_foreground, theme.secondary);
        let lang = Self::lang(cx);
        let issues = v_flex().gap_1().children(self.report.issues.iter().enumerate().map(|(i, issue)| {
            let step = issue.field.step();
            let (icon, color) = if issue.severity == kw_core::Severity::Error {
                (IconName::CircleX, danger)
            } else {
                (IconName::TriangleAlert, warning)
            };
            h_flex()
                .gap_2()
                .items_center()
                .text_sm()
                .child(Icon::new(icon).small().text_color(color))
                .child(div().flex_1().child(issue.message.to_string()))
                .child(
                    Button::new(("goto", i))
                        .xsmall()
                        .ghost()
                        .label(i18n::step_title(lang, step))
                        .icon(IconName::ChevronRight)
                        .on_click(cx.listener(move |this, _, _, cx| this.go(step, cx))),
                )
        }));
        let body = match &self.review {
            Some((_, Ok(files))) => {
                let sel = self.review_sel.min(files.len().saturating_sub(1));
                let preview: SharedString = files.get(sel).map_or_else(String::new, |f| f.text().into_owned()).into();
                h_flex()
                    .h(px(460.))
                    .gap_3()
                    .child(
                        v_flex()
                            .id("review-files")
                            .w(px(300.))
                            .h_full()
                            .overflow_y_scroll()
                            .border_1()
                            .border_color(border)
                            .rounded_md()
                            .p_1()
                            .children(files.iter().enumerate().map(|(i, f)| {
                                Button::new(("rf", i))
                                    .ghost()
                                    .xsmall()
                                    .w_full()
                                    .icon(IconName::File)
                                    .label(f.path.clone())
                                    .selected(i == sel)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.review_sel = i;
                                        cx.notify();
                                    }))
                            })),
                    )
                    .child(
                        div()
                            .id("review-preview")
                            .flex_1()
                            .h_full()
                            .overflow_scroll()
                            .p_2()
                            .rounded_md()
                            .bg(card)
                            .font_family("monospace")
                            .text_xs()
                            .child(preview),
                    )
            }
            Some((_, Err(e))) => h_flex().text_color(danger).child(e.clone()),
            None => h_flex().text_color(muted).child("…"),
        };
        v_flex()
            .gap_3()
            .when(self.report.issues.is_empty(), |d| d.child(div().text_sm().text_color(theme.success).child("✓ No issues")))
            .child(issues)
            .child(body)
    }

    fn step_deploy(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let _ = window;
        let lang = Self::lang(cx);
        let theme = cx.theme();
        let (danger, success, warning, muted, card, border) =
            (theme.danger, theme.success, theme.warning, theme.muted_foreground, theme.secondary, theme.border);
        let busy = self.deploy.busy();
        let has_errors = self.report.has_errors();

        let prereq = v_flex()
            .gap_1()
            .p_3()
            .rounded_md()
            .bg(card)
            .child(
                h_flex().justify_between().child(div().font_semibold().child(i18n::t(lang, K::Prereq))).child(
                    Button::new("recheck").xsmall().ghost().label(i18n::t(lang, K::Recheck)).on_click(cx.listener(|this, _, _, cx| this.check_prereq(cx))),
                ),
            )
            .children(self.deploy.prereq.iter().flatten().map(|s| {
                use kw_exec::prereq::ToolState;
                let (icon, color, text): (IconName, Hsla, String) = match s.state {
                    ToolState::Ok { version: (a, b) } => (IconName::CircleCheck, success, format!("{a}.{b}")),
                    ToolState::Outdated { version: (a, b) } => (IconName::TriangleAlert, warning, format!("{a}.{b} — {}", s.tool.install.current())),
                    ToolState::Missing => (IconName::CircleX, danger, s.tool.install.current().to_owned()),
                };
                h_flex()
                    .gap_2()
                    .items_center()
                    .text_sm()
                    .child(Icon::new(icon).small().text_color(color))
                    .child(div().w(px(90.)).child(s.tool.name))
                    .child(div().text_xs().text_color(muted).child(text))
            }));

        let actions = h_flex()
            .gap_2()
            .flex_wrap()
            .child(
                Button::new("gen")
                    .outline()
                    .icon(IconName::File)
                    .label(i18n::t(lang, K::Generate))
                    .disabled(busy || has_errors)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.generate(cx);
                    })),
            )
            .child(
                Button::new("deploy")
                    .primary()
                    .icon(IconName::Play)
                    .label(i18n::t(lang, K::Deploy))
                    .disabled(busy || has_errors)
                    .when(has_errors, |b| b.tooltip(i18n::t(lang, K::FixErrors)))
                    .on_click(cx.listener(move |_, _, window, cx| {
                        let this = cx.entity().downgrade();
                        window.open_alert_dialog(cx, move |d, _, _| {
                            let this = this.clone();
                            d.title(i18n::t(lang, K::ConfirmDeploy))
                                .description(i18n::t(lang, K::ConfirmDeployBody))
                                .on_ok(move |_, _, cx| {
                                    let _ = this.update(cx, |w, cx| w.start_deploy(cx));
                                    true
                                })
                        });
                    })),
            )
            .child(
                Button::new("demo")
                    .ghost()
                    .icon(IconName::Play)
                    .label(i18n::t(lang, K::Demo))
                    .disabled(busy)
                    .on_click(cx.listener(|this, _, _, cx| this.start_demo(cx))),
            )
            .when(busy, |r| {
                r.child(Button::new("cancel").danger().label(i18n::t(lang, K::Cancel)).on_click(cx.listener(|this, _, _, cx| this.cancel_run(cx))))
            })
            .when(self.deploy.phase == Phase::AwaitApply, |r| {
                r.child(
                    Button::new("apply")
                        .warning()
                        .icon(IconName::Play)
                        .label(i18n::t(lang, K::ConfirmApply))
                        .on_click(cx.listener(move |_, _, window, cx| {
                            let this = cx.entity().downgrade();
                            window.open_alert_dialog(cx, move |d, _, _| {
                                let this = this.clone();
                                d.title(i18n::t(lang, K::ConfirmApply))
                                    .description(i18n::t(lang, K::ConfirmApplyBody))
                                    .on_ok(move |_, _, cx| {
                                        let _ = this.update(cx, |w, cx| w.start_apply(cx));
                                        true
                                    })
                            });
                        })),
                )
            })
            .when(matches!(self.deploy.phase, Phase::Finished { ok: false }) && !self.deploy.demo, |r| {
                r.child(Button::new("retry").outline().label(i18n::t(lang, K::Retry)).on_click(cx.listener(|this, _, _, cx| this.start_apply(cx))))
            });

        let timeline = h_flex().gap_2().flex_wrap().children(self.deploy.steps.iter().map(|(s, st)| {
            let (icon, color, extra): (IconName, Hsla, String) = match st {
                StepState::Pending => (IconName::ChevronRight, muted, String::new()),
                StepState::Running => (IconName::Loader, warning, String::new()),
                StepState::Ok(t) => (IconName::CircleCheck, success, format!(" {t}s")),
                StepState::Failed(t) => (IconName::CircleX, danger, format!(" {t}s")),
            };
            h_flex()
                .gap_1()
                .items_center()
                .px_2()
                .py_1()
                .rounded_md()
                .border_1()
                .border_color(border)
                .text_xs()
                .child(Icon::new(icon).xsmall().text_color(color))
                .child(format!("{}{extra}", s.label()))
        }));

        let errors_only = self.deploy.errors_only;
        let log = v_flex()
            .gap_1()
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .child(div().text_xs().text_color(muted).child(self.deploy.log_file.as_ref().map_or_else(String::new, |p| p.display().to_string())))
                    .child(
                        Checkbox::new("errors-only").label(i18n::t(lang, K::ErrorsOnly)).checked(errors_only).on_change(cx.listener(|this, v: &bool, _, cx| {
                            this.deploy.errors_only = *v;
                            this.deploy.rebuild_log_text();
                            cx.notify();
                        })),
                    ),
            )
            .child(
                div()
                    .id("deploy-log")
                    .h(px(260.))
                    .overflow_scroll()
                    .p_2()
                    .rounded_md()
                    .bg(card)
                    .font_family("monospace")
                    .text_xs()
                    .child(self.deploy.log_text.clone()),
            );

        let can_upgrade = self.existing && !self.spec.topology.ha;
        v_flex()
            .gap_4()
            .child(prereq)
            .child(div().text_sm().child(format!("→ {}", self.project_dir.display())))
            .child(actions)
            .when_some(self.deploy.message.clone(), |d, m| d.child(div().text_sm().child(m)))
            .child(timeline)
            .child(log)
            .when(can_upgrade, |d| {
                d.child(
                    v_flex()
                        .gap_2()
                        .p_3()
                        .rounded_md()
                        .border_1()
                        .border_color(border)
                        .child(div().font_semibold().child(i18n::t(lang, K::UpgradeHa)))
                        .child(div().text_xs().text_color(muted).child(i18n::t(lang, K::UpgradeHaHint)))
                        .child(Textarea::new(&self.deploy.ha_input).h(px(70.)))
                        .child(
                            Button::new("upgrade-ha")
                                .primary()
                                .label(i18n::t(lang, K::UpgradeHa))
                                .disabled(busy)
                                .on_click(cx.listener(|this, _, _, cx| this.start_upgrade_ha(cx))),
                        ),
                )
            })
    }
}

