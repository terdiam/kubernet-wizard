//! In-app documentation: embedded Markdown pages, lazy search index, viewer.

use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::text::TextView;
use gpui_kit::component::{ActiveTheme, IconName, Selectable, Sizable, button::Button, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use smallvec::SmallVec;

use crate::i18n::{self, K};
use crate::settings::Ui;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocPage {
    GettingStarted,
    HighAvailability,
    EndpointLb,
    Network,
    IngressGitOps,
    Tuning,
    Troubleshooting,
}

impl DocPage {
    pub const ALL: [Self; 7] = [
        Self::GettingStarted,
        Self::HighAvailability,
        Self::EndpointLb,
        Self::Network,
        Self::IngressGitOps,
        Self::Tuning,
        Self::Troubleshooting,
    ];

    pub const fn title(self) -> &'static str {
        match self {
            Self::GettingStarted => "Memulai / Getting started",
            Self::HighAvailability => "High Availability",
            Self::EndpointLb => "Endpoint & Load Balancer",
            Self::Network => "Network",
            Self::IngressGitOps => "Ingress & GitOps",
            Self::Tuning => "Tuning",
            Self::Troubleshooting => "Troubleshooting",
        }
    }

    pub const fn source(self) -> &'static str {
        match self {
            Self::GettingStarted => include_str!("../../../docs/getting-started.md"),
            Self::HighAvailability => include_str!("../../../docs/high-availability.md"),
            Self::EndpointLb => include_str!("../../../docs/endpoint-lb.md"),
            Self::Network => include_str!("../../../docs/network.md"),
            Self::IngressGitOps => include_str!("../../../docs/ingress-gitops.md"),
            Self::Tuning => include_str!("../../../docs/tuning.md"),
            Self::Troubleshooting => include_str!("../../../docs/troubleshooting.md"),
        }
    }

    #[inline]
    const fn id(self) -> &'static str {
        match self {
            Self::GettingStarted => "doc-start",
            Self::HighAvailability => "doc-ha",
            Self::EndpointLb => "doc-lb",
            Self::Network => "doc-net",
            Self::IngressGitOps => "doc-ingress",
            Self::Tuning => "doc-tuning",
            Self::Troubleshooting => "doc-trouble",
        }
    }
}

/// Inverted index: lowercase word → pages containing it. Built on first search.
struct Index {
    words: Vec<(String, SmallVec<[u8; 4]>)>,
}

impl Index {
    fn build() -> Self {
        let mut words: Vec<(String, SmallVec<[u8; 4]>)> = Vec::with_capacity(2048);
        for (i, page) in DocPage::ALL.iter().enumerate() {
            let idx = u8::try_from(i).unwrap_or(u8::MAX);
            for w in page.source().split(|c: char| !c.is_alphanumeric() && c != '-').filter(|w| w.len() > 1) {
                let lw = w.to_lowercase();
                match words.binary_search_by(|(k, _)| k.as_str().cmp(&lw)) {
                    Ok(pos) => {
                        let pages = &mut words[pos].1;
                        if !pages.contains(&idx) {
                            pages.push(idx);
                        }
                    }
                    Err(pos) => words.insert(pos, (lw, SmallVec::from_slice(&[idx]))),
                }
            }
        }
        Self { words }
    }

    /// Pages where every query term matches a word prefix.
    fn search(&self, query: &str) -> SmallVec<[DocPage; 7]> {
        let terms: SmallVec<[String; 4]> = query.split_whitespace().map(str::to_lowercase).collect();
        DocPage::ALL
            .iter()
            .enumerate()
            .filter(|(i, _)| {
                terms.iter().all(|t| {
                    let start = self.words.partition_point(|(k, _)| k.as_str() < t.as_str());
                    self.words[start..]
                        .iter()
                        .take_while(|(k, _)| k.starts_with(t.as_str()))
                        .any(|(_, pages)| pages.iter().any(|&p| usize::from(p) == *i))
                })
            })
            .map(|(_, p)| *p)
            .collect()
    }
}

pub struct DocsView {
    page: DocPage,
    search: Entity<InputState>,
    index: Option<Index>,
    hits: Option<SmallVec<[DocPage; 7]>>,
    _subs: [Subscription; 1],
}

impl DocsView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let lang = cx.global::<Ui>().prefs.lang;
        let search = cx.new(|cx| InputState::new(window, cx).placeholder(i18n::t(lang, K::SearchDocs)));
        let sub = cx.subscribe(&search, |this: &mut Self, state, ev: &InputEvent, cx| {
            if matches!(ev, InputEvent::Change) {
                let q = state.read(cx).value();
                this.on_query(&q);
                cx.notify();
            }
        });
        Self { page: DocPage::GettingStarted, search, index: None, hits: None, _subs: [sub] }
    }

    fn on_query(&mut self, q: &str) {
        let q = q.trim();
        if q.is_empty() {
            self.hits = None;
            return;
        }
        let index = self.index.get_or_insert_with(Index::build);
        let hits = index.search(q);
        if let Some(&first) = hits.first()
            && !hits.contains(&self.page)
        {
            self.page = first;
        }
        self.hits = Some(hits);
    }

    pub fn open(&mut self, page: DocPage, cx: &mut Context<Self>) {
        self.page = page;
        cx.notify();
    }
}

impl Render for DocsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui = cx.global::<Ui>();
        let tour_here = ui.tour_target() == Some("docs-search");
        let theme = cx.theme();
        let (ring, border, muted) = (theme.ring, theme.border, theme.muted_foreground);
        let visible: SmallVec<[DocPage; 7]> =
            self.hits.clone().unwrap_or_else(|| SmallVec::from_slice(&DocPage::ALL));

        h_flex()
            .size_full()
            .child(
                v_flex()
                    .w(px(260.))
                    .h_full()
                    .p_3()
                    .gap_2()
                    .border_r_1()
                    .border_color(border)
                    .child(
                        div()
                            .id("docs-search")
                            .rounded_md()
                            .when(tour_here, |d| d.border_2().border_color(ring))
                            .child(Input::new(&self.search).small()),
                    )
                    .when(visible.is_empty(), |d| d.child(div().text_sm().text_color(muted).child("—")))
                    .children(visible.into_iter().map(|p| {
                        Button::new(p.id())
                            .ghost()
                            .small()
                            .w_full()
                            .label(p.title())
                            .icon(IconName::BookOpen)
                            .selected(p == self.page)
                            .on_click(cx.listener(move |this, _, _, cx| this.open(p, cx)))
                    })),
            )
            .child(
                div()
                    .id("docs-body")
                    .flex_1()
                    .h_full()
                    .p_6()
                    .overflow_y_scroll()
                    .child(TextView::markdown(self.page.id(), self.page.source())),
            )
    }
}

use gpui_kit::component::button::ButtonVariants as _;

#[cfg(test)]
mod tests {
    use super::{DocPage, Index};

    #[test]
    fn search_finds_pages() {
        let idx = Index::build();
        assert!(idx.search("kube-vip").contains(&DocPage::EndpointLb));
        assert!(idx.search("etcd quorum").contains(&DocPage::HighAvailability));
        assert!(idx.search("zzzz-not-here").is_empty());
    }
}
