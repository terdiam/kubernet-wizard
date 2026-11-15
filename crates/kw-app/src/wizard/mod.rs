//! Wizard state: the spec being edited, validation, inputs, navigation.

mod deploy;
mod steps;
mod view;

use std::borrow::Cow;
use std::collections::VecDeque;
use std::path::PathBuf;

use gpui_kit::component::input::{InputEvent, InputState, TextareaState};
use gpui_kit::*;
use kw_core::{
    ClusterSpec, Field, Issue, Node, NodeRole, Provisioning, Report, Severity, Step, Target,
};
use kw_render::RenderedFile;
use smallvec::SmallVec;

use crate::docs::DocPage;
use crate::help::Topic;

pub use deploy::DeployState;

/// Text inputs, one retained `InputState` each (indexed by discriminant).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum In {
    Name,
    Region,
    CpSize,
    WorkerSize,
    Image,
    Vip,
    Iface,
    DnsName,
    Pool,
    PodCidr,
    SvcCidr,
    NodeCidr,
    Domain,
    Mtu,
    RepoUrl,
    Branch,
    GitPath,
    NodeMem,
}

const INPUTS: [In; 18] = [
    In::Name,
    In::Region,
    In::CpSize,
    In::WorkerSize,
    In::Image,
    In::Vip,
    In::Iface,
    In::DnsName,
    In::Pool,
    In::PodCidr,
    In::SvcCidr,
    In::NodeCidr,
    In::Domain,
    In::Mtu,
    In::RepoUrl,
    In::Branch,
    In::GitPath,
    In::NodeMem,
];

impl In {
    const fn field(self) -> Option<Field> {
        Some(match self {
            Self::Name => Field::Name,
            Self::Region | Self::CpSize | Self::WorkerSize | Self::Image => Field::Provisioning,
            Self::Vip | Self::DnsName => Field::Vip,
            Self::Iface => Field::Interface,
            Self::Pool => Field::LbPool,
            Self::PodCidr => Field::PodCidr,
            Self::SvcCidr => Field::ServiceCidr,
            Self::NodeCidr => Field::NodeCidr,
            Self::Mtu => Field::Mtu,
            Self::RepoUrl | Self::Branch | Self::GitPath => Field::GitOpsRepo,
            Self::Domain | Self::NodeMem => return None,
        })
    }

    const fn placeholder(self) -> &'static str {
        match self {
            Self::Name => "prod-cluster",
            Self::Region => "fsn1",
            Self::CpSize => "cpx21",
            Self::WorkerSize => "cpx31",
            Self::Image => "ubuntu-24.04",
            Self::Vip => "192.168.1.100",
            Self::Iface => "eth0",
            Self::DnsName => "k8s.example.com",
            Self::Pool => "192.168.1.240-192.168.1.250",
            Self::PodCidr => "10.42.0.0/16",
            Self::SvcCidr => "10.43.0.0/16",
            Self::NodeCidr => "192.168.1.0/24",
            Self::Domain => "cluster.local",
            Self::Mtu => "auto",
            Self::RepoUrl => "git@github.com:org/gitops.git",
            Self::Branch => "main",
            Self::GitPath => "clusters/<name>",
            Self::NodeMem => "4",
        }
    }
}

fn tofu_fields(spec: &ClusterSpec) -> Option<[&str; 4]> {
    match &spec.target {
        Target::Cloud { provisioning: Provisioning::Tofu { region, cp_size, worker_size, image }, .. } => {
            Some([region, cp_size, worker_size, image])
        }
        _ => None,
    }
}

fn opt_to_string<T: ToString>(v: Option<T>) -> Cow<'static, str> {
    v.map_or(Cow::Borrowed(""), |v| Cow::Owned(v.to_string()))
}

/// Current spec value shown in an input.
fn value_of(spec: &ClusterSpec, input: In) -> Cow<'_, str> {
    let tofu = tofu_fields(spec);
    let tf = |i: usize| tofu.map_or(Cow::Borrowed(""), |t| Cow::Borrowed(t[i]));
    match input {
        In::Name => Cow::Borrowed(&spec.name),
        In::Region => tf(0),
        In::CpSize => tf(1),
        In::WorkerSize => tf(2),
        In::Image => tf(3),
        In::Vip => opt_to_string(spec.endpoint.vip),
        In::Iface => Cow::Borrowed(spec.endpoint.interface.as_deref().unwrap_or("")),
        In::DnsName => Cow::Borrowed(spec.endpoint.dns_name.as_deref().unwrap_or("")),
        In::Pool => Cow::Borrowed(spec.service_lb.pool.as_deref().unwrap_or("")),
        In::PodCidr => Cow::Owned(spec.network.pod_cidr.to_string()),
        In::SvcCidr => Cow::Owned(spec.network.service_cidr.to_string()),
        In::NodeCidr => opt_to_string(spec.network.node_cidr),
        In::Domain => Cow::Borrowed(&spec.network.cluster_domain),
        In::Mtu => opt_to_string(spec.network.mtu),
        In::RepoUrl => Cow::Borrowed(&spec.gitops.repo_url),
        In::Branch => Cow::Borrowed(&spec.gitops.branch),
        In::GitPath => Cow::Borrowed(&spec.gitops.path),
        In::NodeMem => opt_to_string(spec.tuning.node_memory_gib),
    }
}

#[inline]
fn non_empty(v: &str) -> Option<String> {
    let v = v.trim();
    (!v.is_empty()).then(|| v.to_owned())
}

/// Applies input text to the spec. `Err` = parse error message.
fn apply(spec: &mut ClusterSpec, input: In, v: &str) -> Result<(), &'static str> {
    let t = v.trim();
    let set_tofu = |spec: &mut ClusterSpec, i: usize| {
        if let Target::Cloud { provisioning: Provisioning::Tofu { region, cp_size, worker_size, image }, .. } = &mut spec.target {
            let slot = [region, cp_size, worker_size, image].into_iter().nth(i);
            if let Some(s) = slot {
                t.clone_into(s);
            }
        }
    };
    match input {
        In::Name => t.clone_into(&mut spec.name),
        In::Region => set_tofu(spec, 0),
        In::CpSize => set_tofu(spec, 1),
        In::WorkerSize => set_tofu(spec, 2),
        In::Image => set_tofu(spec, 3),
        In::Vip => spec.endpoint.vip = if t.is_empty() { None } else { Some(t.parse().map_err(|_| "Not a valid IP address")?) },
        In::Iface => spec.endpoint.interface = non_empty(t),
        In::DnsName => spec.endpoint.dns_name = non_empty(t),
        In::Pool => spec.service_lb.pool = non_empty(t),
        In::PodCidr => spec.network.pod_cidr = t.parse().map_err(|_| "Use CIDR notation, e.g. 10.42.0.0/16")?,
        In::SvcCidr => spec.network.service_cidr = t.parse().map_err(|_| "Use CIDR notation, e.g. 10.43.0.0/16")?,
        In::NodeCidr => {
            spec.network.node_cidr = if t.is_empty() { None } else { Some(t.parse().map_err(|_| "Use CIDR notation, e.g. 192.168.1.0/24")?) };
        }
        In::Domain => t.clone_into(&mut spec.network.cluster_domain),
        In::Mtu => spec.network.mtu = if t.is_empty() { None } else { Some(t.parse().map_err(|_| "MTU must be a number")?) },
        In::RepoUrl => t.clone_into(&mut spec.gitops.repo_url),
        In::Branch => t.clone_into(&mut spec.gitops.branch),
        In::GitPath => t.clone_into(&mut spec.gitops.path),
        In::NodeMem => spec.tuning.node_memory_gib = if t.is_empty() { None } else { Some(t.parse().map_err(|_| "RAM in GiB, e.g. 8")?) },
    }
    Ok(())
}

/// `name ip role [user] [key]` per line.
pub fn parse_nodes(text: &str) -> Result<Vec<Node>, String> {
    let mut nodes = Vec::with_capacity(text.lines().count());
    for (n, line) in text.lines().enumerate().map(|(i, l)| (i + 1, l.trim())) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let (Some(name), Some(ip)) = (parts.next(), parts.next()) else {
            return Err(format!("Line {n}: expected `name ip role [user] [key]`"));
        };
        let address = ip.parse().map_err(|_| format!("Line {n}: '{ip}' is not an IP address"))?;
        let role = match parts.next().unwrap_or("worker") {
            "cp" | "control-plane" | "control_plane" | "master" | "server" => NodeRole::ControlPlane,
            "worker" | "w" | "agent" => NodeRole::Worker,
            other => return Err(format!("Line {n}: role '{other}' must be cp or worker")),
        };
        nodes.push(Node {
            name: name.to_owned(),
            address,
            role,
            ssh_user: parts.next().unwrap_or("root").to_owned(),
            ssh_key: parts.next().map(str::to_owned),
        });
    }
    Ok(nodes)
}

pub fn format_nodes(nodes: &[Node]) -> String {
    let mut out = String::with_capacity(nodes.len() * 48);
    for n in nodes {
        use std::fmt::Write as _;
        let role = if n.role == NodeRole::ControlPlane { "cp" } else { "worker" };
        let _ = write!(out, "{} {} {role} {}", n.name, n.address, n.ssh_user);
        if let Some(k) = &n.ssh_key {
            out.push(' ');
            out.push_str(k);
        }
        out.push('\n');
    }
    out
}

pub enum WizardEvent {
    OpenDocs(DocPage),
    GoHome,
}

impl EventEmitter<WizardEvent> for Wizard {}

const HISTORY_CAP: usize = 50;

pub struct Wizard {
    spec: ClusterSpec,
    report: Report,
    parse_errs: SmallVec<[(Field, Cow<'static, str>); 4]>,
    step: Step,
    visited: u16,
    rev: u64,
    inputs: [Entity<InputState>; 18],
    nodes: Entity<TextareaState>,
    topic: Option<Topic>,
    undo: VecDeque<ClusterSpec>,
    redo: Vec<ClusterSpec>,
    /// Folder the project is written to.
    project_dir: PathBuf,
    /// Spec was loaded from an existing project (enables Day-2 actions).
    existing: bool,
    review: Option<(u64, Result<Vec<RenderedFile>, String>)>,
    review_sel: usize,
    deploy: DeployState,
    /// Set while code updates inputs, so their Change events are ignored.
    syncing: bool,
    _subs: Vec<Subscription>,
}

impl Wizard {
    pub fn new(spec: ClusterSpec, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let inputs: [Entity<InputState>; 18] = std::array::from_fn(|i| {
            let key = INPUTS[i];
            let value = value_of(&spec, key).into_owned();
            cx.new(|cx| InputState::new(window, cx).placeholder(key.placeholder()).default_value(value))
        });
        let nodes_text = format_nodes(&spec.topology.nodes);
        let nodes = cx.new(|cx| {
            TextareaState::new(window, cx)
                .rows(6)
                .placeholder("cp-1 192.168.1.11 cp root ~/.ssh/id_ed25519")
                .default_value(nodes_text)
        });
        let ha_input = cx.new(|cx| {
            TextareaState::new(window, cx).rows(3).placeholder("cp-2 192.168.1.12 cp root\ncp-3 192.168.1.13 cp root")
        });

        let mut subs = Vec::with_capacity(INPUTS.len() + 1);
        for (i, input) in inputs.iter().enumerate() {
            let key = INPUTS[i];
            subs.push(cx.subscribe(input, move |this: &mut Self, state, ev: &InputEvent, cx| {
                if matches!(ev, InputEvent::Change) && !this.syncing {
                    let v = state.read(cx).value();
                    this.on_input(key, &v, cx);
                }
            }));
        }
        subs.push(cx.subscribe(&nodes, |this: &mut Self, state, ev: &InputEvent, cx| {
            if matches!(ev, InputEvent::Change) && !this.syncing {
                let v = state.read(cx).value();
                this.on_nodes(&v, cx);
            }
        }));

        let project_dir = crate::settings::projects_dir().join(&spec.name);
        let mut this = Self {
            report: Report::default(),
            parse_errs: SmallVec::new(),
            step: Step::Project,
            visited: 1,
            rev: 0,
            inputs,
            nodes,
            topic: None,
            undo: VecDeque::with_capacity(HISTORY_CAP),
            redo: Vec::new(),
            project_dir,
            existing: false,
            review: None,
            review_sel: 0,
            deploy: DeployState::new(ha_input),
            syncing: false,
            _subs: subs,
            spec,
        };
        this.revalidate();
        this
    }

    // ------------------------------------------------------------ spec edits

    fn revalidate(&mut self) {
        self.report = kw_distro::validate(&self.spec);
        self.report.issues.extend(self.parse_errs.iter().map(|(field, msg)| Issue {
            field: *field,
            severity: Severity::Error,
            message: msg.clone(),
        }));
        self.rev = self.rev.wrapping_add(1);
    }

    /// Discrete edit (chip, switch, checkbox): recorded in undo history.
    pub fn edit(&mut self, f: impl FnOnce(&mut ClusterSpec), cx: &mut Context<Self>) {
        if self.undo.len() == HISTORY_CAP {
            self.undo.pop_front();
        }
        self.undo.push_back(self.spec.clone());
        self.redo.clear();
        f(&mut self.spec);
        self.revalidate();
        cx.notify();
    }

    fn on_input(&mut self, key: In, v: &str, cx: &mut Context<Self>) {
        let field = key.field();
        self.parse_errs.retain(|(f, _)| Some(*f) != field);
        if let Err(msg) = apply(&mut self.spec, key, v)
            && let Some(f) = field
        {
            self.parse_errs.push((f, Cow::Borrowed(msg)));
        }
        if key == In::Name {
            self.project_dir = crate::settings::projects_dir().join(&self.spec.name);
        }
        self.revalidate();
        cx.notify();
    }

    fn on_nodes(&mut self, v: &str, cx: &mut Context<Self>) {
        self.parse_errs.retain(|(f, _)| *f != Field::Nodes);
        match parse_nodes(v) {
            Ok(nodes) => self.spec.topology.nodes = nodes,
            Err(msg) => self.parse_errs.push((Field::Nodes, Cow::Owned(msg))),
        }
        self.revalidate();
        cx.notify();
    }

    /// Pushes spec values into all inputs (after preset / load / undo).
    fn sync_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.syncing = true;
        for (i, input) in self.inputs.iter().enumerate() {
            let v: SharedString = value_of(&self.spec, INPUTS[i]).into_owned().into();
            input.update(cx, |s, cx| s.set_value(v, window, cx));
        }
        let text: SharedString = format_nodes(&self.spec.topology.nodes).into();
        self.nodes.update(cx, |s, cx| s.set_value(text, window, cx));
        self.syncing = false;
        self.parse_errs.clear();
    }

    pub fn load(&mut self, spec: ClusterSpec, dir: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) {
        self.existing = dir.is_some();
        self.project_dir = dir.unwrap_or_else(|| crate::settings::projects_dir().join(&spec.name));
        self.spec = spec;
        self.undo.clear();
        self.redo.clear();
        self.step = Step::Project;
        self.visited = 1;
        self.review = None;
        self.sync_inputs(window, cx);
        self.revalidate();
        cx.notify();
    }

    pub fn undo(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(prev) = self.undo.pop_back() {
            self.redo.push(std::mem::replace(&mut self.spec, prev));
            self.sync_inputs(window, cx);
            self.revalidate();
            cx.notify();
        }
    }

    pub fn redo(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(next) = self.redo.pop() {
            self.undo.push_back(std::mem::replace(&mut self.spec, next));
            self.sync_inputs(window, cx);
            self.revalidate();
            cx.notify();
        }
    }

    /// A deploy/upgrade run is in progress (blocks installing app updates).
    #[inline]
    pub fn is_busy(&self) -> bool {
        self.deploy.busy()
    }

    #[inline]
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    #[inline]
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    // ------------------------------------------------------------ navigation

    pub fn go(&mut self, step: Step, cx: &mut Context<Self>) {
        self.step = step;
        self.visited |= 1 << step.index();
        self.topic = None;
        if matches!(step, Step::Review | Step::Deploy) {
            self.ensure_review();
        }
        if step == Step::Deploy && self.deploy.prereq.is_none() {
            self.deploy.prereq = Some(Vec::new());
            self.check_prereq(cx);
        }
        self.autosave();
        cx.notify();
    }

    fn step_offset(&self, delta: isize) -> Option<Step> {
        self.step.index().checked_add_signed(delta).and_then(|i| Step::ALL.get(i).copied())
    }

    /// Errors in the current step block Next.
    pub fn step_blocked(&self) -> bool {
        self.report.step_severity(self.step) == Some(Severity::Error)
    }

    fn ensure_review(&mut self) {
        if self.review.as_ref().is_some_and(|(r, _)| *r == self.rev) {
            return;
        }
        let files = kw_render::render_spec(&self.spec).map_err(|e| e.to_string());
        self.review_sel = files.as_ref().ok().and_then(|f| f.iter().position(|x| x.path == "ansible/site.yml")).unwrap_or(0);
        self.review = Some((self.rev, files));
    }

    /// Draft autosave of cluster.yaml for existing projects only (never
    /// creates folders silently).
    fn autosave(&self) {
        if self.existing
            && let Ok(text) = self.spec.to_yaml()
        {
            let _ = std::fs::write(self.project_dir.join("cluster.yaml"), text);
        }
    }

    #[inline]
    fn issues_for(&self, field: Field) -> impl Iterator<Item = &Issue> {
        self.report.for_field(field)
    }
}
