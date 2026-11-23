//! The block every primary timeline opens with: write, then switch between timelines.

use std::sync::Arc;

use gloo_timers::future::TimeoutFuture;
use leptos::prelude::*;
use leptos::task::spawn_local;
use nodus_client_sdk::Timeline;

use crate::routes::compose::inline::InlineComposer;
use crate::routes::home::PrimaryTabs;

/// The composer and the timeline switch, in that order, above the posts.
///
/// One block used by all four primary timelines, so a reader who arrives on Local or
/// Federated still has the way back and the way to write. The composer's own destination is
/// always Home: a post is published to the account, and Home is where its author will look
/// for it, so a refresh after publishing belongs there whatever screen the box sits on.
///
/// The first frame is deliberately the shared [`nodus_pages::…::ComposerSlot`] preview:
/// that is the tree the server painted, so `hydrate_from` can adopt it. The effect flips
/// the slot to the live composer only after the adopted document is in place; the composer
/// itself is unchanged, it is simply the second frame instead of the first.
#[component]
pub fn TimelineHeader() -> impl IntoView {
    let (_reload, set_reload) = signal(0_u32);
    let publish_to = StoredValue::new(Timeline::Home);
    let (live, set_live) = signal(false);
    // `hydrate_from` walks the tree synchronously. Flipping the slot in an effect can run
    // while that walk is still in progress, which replaces the nodes it is about to adopt.
    // One turn of the task queue later the adopted tree is in place.
    spawn_local(async move {
        TimeoutFuture::new(0).await;
        set_live.set(true);
    });

    let active = Signal::derive(move || crate::router::route().get().path());
    let composer: ChildrenFn = Arc::new(move || {
        view! {
            <div class="flex flex-col gap-2">
                <InlineComposer which=Some(publish_to) set_reload=Some(set_reload)/>
                <PrimaryTabs/>
            </div>
        }
        .into_any()
    });

    view! {
        <nodus_pages::pages::compose::inline::ComposerSlot
            active=active
            live=Signal::from(live)
            composer=composer
        />
    }
}
