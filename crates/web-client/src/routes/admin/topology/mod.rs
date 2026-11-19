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

/// How long a drawer reads on a timer when the live stream cannot be held.
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
    // Set only when the live stream refuses: the timer is the fallback, not the way.
    let (polling, set_polling) = signal(false);
    // The tail the open drawer holds, so closing it or leaving the screen hangs up.
    // Local storage: the tail is a browser reader, which is neither `Send` nor `Sync`.
    let tail = StoredValue::new_local(None::<admin::LogTail>);
    on_cleanup(move || {
        tail.update_value(|held| {
            if let Some(tail) = held.take() {
                tail.stop();
            }
        });
    });

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

    // Following: the stream is the way, and it is hung up the moment the drawer closes,
    // the level changes, or the screen goes away.
    Effect::new(move |_| {
        tail.update_value(|held| {
            if let Some(previous) = held.take() {
                previous.stop();
            }
        });
        set_polling.set(false);
        let Some(name) = selected.get() else {
            return;
        };
        if !live.get() {
            return;
        }
        let level = min_level.get();
        let streamed = admin::LogTail::new();
        tail.set_value(Some(streamed.clone()));
        let alive = Alive::new();
        spawn_local(async move {
            let ended = streamed
                .follow(&name, &level, LOG_LIMIT, |entry| {
                    alive.if_alive(|| append_line(set_logs, entry));
                })
                .await;
            if let Err(message) = ended {
                alive.if_alive(|| {
                    set_error.set(Some(message));
                    // The stream could not be held, so read the drawer on a timer instead.
                    set_polling.set(true);
                });
            }
        });
    });

    Effect::new(move |_| {
        if !polling.get() {
            return;
        }
        let Some(name) = selected.get() else {
            return;
        };
        let level = min_level.get();
        let alive = Alive::new();
        spawn_local(async move {
            loop {
                TimeoutFuture::new(FOLLOW_GAP_MS).await;
                if !alive.is_alive()
                    || !live.get_untracked()
                    || selected.get_untracked().as_deref() != Some(name.as_str())
                {
                    break;
                }
                match admin::node_logs(&name, &level, LOG_LIMIT).await {
                    Ok(list) => alive.if_alive(|| set_logs.set(list)),
                    Err(message) => {
                        alive.if_alive(|| {
                            set_error.set(Some(message));
                            set_live.set(false);
                        });
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
                set_polling.set(false);
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

/// Adds one streamed line, keeping the drawer's list to its last [`admin::HELD_LINES`].
///
/// The stream opens by re-sending the tail the drawer has just read, so a line it already
/// shows is skipped: the list is keyed by timestamp and message, and a repeated key is a
/// diff the page cannot render.
fn append_line(set_logs: WriteSignal<Vec<NodeLog>>, entry: NodeLog) {
    set_logs.update(|held| {
        if held
            .iter()
            .any(|seen| seen.timestamp == entry.timestamp && seen.message == entry.message)
        {
            return;
        }
        held.push(entry);
        let over = held.len().saturating_sub(admin::HELD_LINES);
        if over > 0 {
            held.drain(..over);
        }
    });
}
