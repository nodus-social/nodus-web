//! The instance's own processes, asked on mount and when a button says so.

use gloo_timers::future::TimeoutFuture;
use leptos::prelude::*;
use leptos::task::spawn_local;
use nodus_client_core::model::{NodeLog, Topology};
use nodus_pages::pages::admin::topology::TopologyPage;

use crate::state::alive::Alive;
use crate::state::bridge::{admin, revision};

/// How many entries a log drawer reads at a time.
const LOG_LIMIT: u16 = 200;

/// How long a following drawer waits between reads.
const FOLLOW_GAP_MS: u32 = 2_000;

#[component]
pub fn System() -> impl IntoView {
    let revision = revision();
    let (topology, set_topology) = signal(None::<Topology>);
    let (logs, set_logs) = signal(Vec::<NodeLog>::new());
    let (selected, set_selected) = signal(None::<String>);
    let (min_level, set_min_level) = signal("info".to_owned());
    let (busy, set_busy) = signal(false);
    let (error, set_error) = signal(None::<String>);
    let (asked, set_asked) = signal(0_u64);
    let (live, set_live) = signal(false);
    let (notice, set_notice) = signal(None::<String>);

    Effect::new(move |_| {
        revision.get();
        asked.get();
        let alive = Alive::new();
        set_busy.set(true);
        spawn_local(async move {
            let answer = admin::topology().await;
            alive.if_alive(|| {
                set_busy.set(false);
                match answer {
                    Ok(topology) => {
                        set_topology.set(Some(topology));
                        set_error.set(None);
                    }
                    Err(message) => set_error.set(Some(message)),
                }
            });
        });
    });

    Effect::new(move |_| {
        let Some(name) = selected.get() else {
            return;
        };
        let level = min_level.get();
        let alive = Alive::new();
        spawn_local(async move {
            match admin::node_logs(&name, &level, LOG_LIMIT).await {
                Ok(list) => alive.if_alive(|| set_logs.set(list)),
                Err(message) => alive.if_alive(|| set_error.set(Some(message))),
            }
        });
    });

    Effect::new(move |_| {
        if !live.get() {
            return;
        }
        let Some(name) = selected.get() else {
            return;
        };
        let level = min_level.get();
        spawn_local(async move {
            loop {
                TimeoutFuture::new(FOLLOW_GAP_MS).await;
                if !live.get_untracked() || selected.get_untracked().as_deref() != Some(name.as_str()) {
                    break;
                }
                match admin::node_logs(&name, &level, LOG_LIMIT).await {
                    Ok(list) => set_logs.set(list),
                    Err(message) => {
                        set_error.set(Some(message));
                        set_live.set(false);
                        break;
                    }
                }
            }
        });
    });

    view! {
        <TopologyPage
            topology=topology
            logs=logs
            selected=selected
            min_level=min_level
            live=live
            busy=busy
            error=error
            on_refresh=Callback::new(move |()| {
                set_asked.update(|count| *count += 1);
            })
            on_open=Callback::new(move |name: String| {
                set_logs.set(Vec::new());
                set_selected.set(Some(name));
            })
            on_close=Callback::new(move |()| {
                set_live.set(false);
                set_selected.set(None);
            })
            notice=notice
            on_trigger=Callback::new(move |(name, work): (String, String)| {
                spawn_local(async move {
                    match admin::trigger(&name, &work).await {
                        Ok(answer) if answer.accepted => set_notice.set(Some(format!(
                            "{name} took the request: {}",
                            answer.detail,
                        ))),
                        Ok(answer) => set_notice.set(Some(format!("{name} said no: {}", answer.detail))),
                        Err(message) => set_notice.set(Some(format!("{name} could not be asked: {message}"))),
                    }
                });
            })
            on_follow=Callback::new(move |follow: bool| set_live.set(follow))
            on_level=Callback::new(move |level: String| {
                set_logs.set(Vec::new());
                set_min_level.set(level);
            })
        />
    }
}
