//! The block every primary timeline opens with: write, then switch between timelines.

use leptos::prelude::*;
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
/// The first frame is the live composer: the server painted the same
/// [`nodus_pages::…::StaticComposer`] tree, so `hydrate_from` adopts it and no node is
/// replaced afterwards. What changes after hydration is what the controls *do*, not which
/// nodes exist.
#[component]
pub fn TimelineHeader() -> impl IntoView {
    let (_reload, set_reload) = signal(0_u32);
    let publish_to = StoredValue::new(Timeline::Home);

    view! {
        <div class="flex flex-col gap-2">
            <InlineComposer which=Some(publish_to) set_reload=Some(set_reload)/>
            <PrimaryTabs/>
        </div>
    }
}
