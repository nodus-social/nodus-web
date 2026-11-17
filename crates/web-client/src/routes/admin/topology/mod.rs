//! The instance's own processes, asked on mount and when a button says so.

use leptos::prelude::*;
use leptos::task::spawn_local;
use nodus_client_core::model::{NodeLog, Topology};
use nodus_pages::pages::admin::topology::TopologyPage;

use crate::state::alive::Alive;
use crate::state::bridge::{admin, revision};

/// How many entries a log drawer reads at a time.
const LOG_LIMIT: u16 = 200;

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

    view! {
        <TopologyPage
            topology=topology
            logs=logs
            selected=selected
            min_level=min_level
            busy=busy
            error=error
            on_refresh=Callback::new(move |()| {
                set_asked.update(|count| *count += 1);
            })
            on_open=Callback::new(move |name: String| {
                set_logs.set(Vec::new());
                set_selected.set(Some(name));
            })
            on_close=Callback::new(move |()| set_selected.set(None))
            on_level=Callback::new(move |level: String| {
                set_logs.set(Vec::new());
                set_min_level.set(level);
            })
        />
    }
}
