//! Search across accounts, posts and hashtags.

use std::sync::Arc;

use leptos::prelude::*;
use nodus_client_core::model::Status;
use nodus_state::page::{PageItems, PageKind};

use crate::components::CardRenderer;
use crate::pages::search::SearchPage;
use crate::relative_age;

use super::super::Runtime;

#[component]
pub fn SearchScreen(runtime: Arc<dyn Runtime>, card: CardRenderer) -> impl IntoView {
    let revision = runtime.revision();
    let seed = runtime.query("q").unwrap_or_default();
    let asked = RwSignal::new(
        if seed.trim().is_empty() || runtime.page(&search(&seed)).is_none() {
            String::new()
        } else {
            seed.clone()
        },
    );
    let query = RwSignal::new(seed.clone());
    let loading = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let alive = crate::Alive::new();

    let found = Signal::derive({
        let runtime = Arc::clone(&runtime);
        move || {
            revision.get();
            let text = asked.get();
            if text.trim().is_empty() {
                return None;
            }
            match runtime.page(&search(&text))?.items {
                PageItems::Search(results) => Some(*results),
                _ => None,
            }
        }
    });
    let accounts =
        Signal::derive(move || found.get().map(|found| found.accounts).unwrap_or_default());
    let statuses =
        Signal::derive(move || found.get().map(|found| found.statuses).unwrap_or_default());
    let tags = Signal::derive(move || found.get().map(|found| found.hashtags).unwrap_or_default());
    let searched = Signal::derive(move || !asked.get().trim().is_empty());

    let ask: Callback<()> = Callback::new({
        let runtime = Arc::clone(&runtime);
        move |()| {
            let text = query.get_untracked().trim().to_owned();
            if text.is_empty() {
                return;
            }
            loading.set(true);
            error.set(None);
            let asked_text = text.clone();
            let done = Callback::new(move |result: Result<(), String>| {
                alive.if_alive(|| {
                    loading.set(false);
                    match result {
                        Ok(()) => asked.set(asked_text.clone()),
                        Err(message) => error.set(Some(message)),
                    }
                });
            });
            runtime.load_page(search(&text), None, done);
        }
    });

    if !seed.trim().is_empty() && runtime.page(&search(&seed)).is_none() {
        ask.run(());
    }

    let on_query = Callback::new(move |text: String| query.set(text));
    let age_of = Callback::new({
        let runtime = Arc::clone(&runtime);
        move |created: String| relative_age(&created, runtime.now_millis())
    });
    let card = Callback::new(move |status: Status| card(status));

    view! {
        <SearchPage
            query=query.into()
            accounts=accounts
            statuses=statuses
            tags=tags
            searched=searched
            loading=loading.into()
            error=error.into()
            on_query=on_query
            on_search=ask
            age_of=Some(age_of)
            card=Some(card)
        />
    }
}

fn search(query: &str) -> PageKind {
    PageKind::Search(query.to_owned())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use leptos::prelude::{Callback, IntoAny as _, Owner, RenderHtml as _, Signal};
    use nodus_client_core::model::{SearchResults, Status, Tag};
    use nodus_state::page::{PageEntry, PageItems, PageKind};

    use super::{SearchScreen, SearchScreenProps};
    use crate::components::CardRenderer;
    use crate::route::{Runtime, TimelineKind, TimelineReply};

    struct PrefetchedSearchRuntime;

    impl Runtime for PrefetchedSearchRuntime {
        fn revision(&self) -> Signal<u64> {
            Signal::derive(|| 0)
        }

        fn now_millis(&self) -> f64 {
            0.0
        }

        fn timeline(&self, _kind: &TimelineKind) -> TimelineReply {
            TimelineReply::unavailable()
        }

        fn load_timeline(
            &self,
            _kind: TimelineKind,
            _cursor: Option<String>,
            _done: Callback<Result<(), String>>,
        ) {
        }

        fn page(&self, kind: &PageKind) -> Option<PageEntry> {
            match kind {
                PageKind::Search(q) if q == "#rust" => Some(PageEntry {
                    items: PageItems::Search(Box::new(SearchResults {
                        accounts: Vec::new(),
                        statuses: Vec::new(),
                        hashtags: vec![Tag {
                            name: "rust".to_owned(),
                            url: "https://example.org/tags/rust".to_owned(),
                            history: Vec::new(),
                        }],
                    })),
                    cursor: None,
                    fetched_at: None,
                }),
                _ => None,
            }
        }

        fn query(&self, key: &str) -> Option<String> {
            (key == "q").then(|| "#rust".to_owned())
        }
    }

    #[test]
    fn prefetched_search_does_not_fail_on_ssr() {
        let card: CardRenderer = Arc::new(|_status: Status| ().into_any());
        let html = Owner::new().with(|| {
            SearchScreen(SearchScreenProps {
                runtime: Arc::new(PrefetchedSearchRuntime),
                card,
            })
            .to_html()
        });
        assert!(html.contains("#rust"), "{html}");
        assert!(
            !html.contains("This page is not available without the app."),
            "{html}"
        );
    }
}
