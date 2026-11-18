//! A chart, drawn by the bundled Chart.js wrapper.
//!
//! The page owns the numbers and the wording; the bundle owns the ink, which it reads from
//! the page's theme variables. Nothing here touches the DOM on the server: the same props
//! paint as an empty box, so hydration mounts the drawing into it.

use leptos::prelude::*;
use serde::Serialize;

/// How a chart draws its series unless a series says otherwise.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// A line per series.
    #[default]
    Line,
    /// A bar per label, per series.
    Bar,
}

/// One line of numbers.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Series {
    /// The word in the legend.
    pub label: String,
    /// One number per label; a short series is padded by the drawing, not here.
    pub values: Vec<f64>,
    /// Overrides the chart's kind, which is what a backlog line does on a bar chart.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<Kind>,
}

impl Series {
    /// A series drawn like the chart it is in.
    #[must_use]
    pub fn new(label: impl Into<String>, values: Vec<f64>) -> Self {
        Self {
            label: label.into(),
            values,
            kind: None,
        }
    }

    /// A series drawn as a line whatever the chart is.
    #[must_use]
    pub fn line(label: impl Into<String>, values: Vec<f64>) -> Self {
        Self {
            kind: Some(Kind::Line),
            ..Self::new(label, values)
        }
    }
}

/// What a chart draws: `labels.len()` slots per series.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Spec {
    /// The drawing the series share.
    pub kind: Kind,
    /// The words under the axis, oldest first.
    pub labels: Vec<String>,
    /// The numbers, in the order the legend shows them.
    pub series: Vec<Series>,
    /// Whether the value axis starts at zero; a backlog line usually should.
    pub begin_at_zero: bool,
    /// Whether the axis counts whole things, so it stops writing `2.5`.
    pub integer: bool,
}

impl Spec {
    /// A chart with a zero-based, whole-number axis.
    #[must_use]
    pub fn new(kind: Kind, labels: Vec<String>, series: Vec<Series>) -> Self {
        Self {
            kind,
            labels,
            series,
            begin_at_zero: true,
            integer: true,
        }
    }

    /// Whether there is nothing to draw, so the caller's empty sentence is the honest paint.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.labels.is_empty() || self.series.iter().all(|series| series.values.is_empty())
    }
}

/// A chart with a caption, an empty state and a way back from a bundle that failed.
#[component]
pub fn Chart(
    /// `None` while the numbers are still being read; an empty spec is a real empty answer.
    #[prop(into)]
    spec: Signal<Option<Spec>>,
    /// What to say when the numbers arrived and there are none.
    #[prop(into)]
    empty: String,
    /// What the drawing is of, read above it.
    #[prop(optional, into)]
    caption: Option<String>,
) -> impl IntoView {
    let element = NodeRef::<leptos::html::Div>::new();
    let (failed, set_failed) = signal(false);
    let (attempt, set_attempt) = signal(0_u32);

    #[cfg(target_arch = "wasm32")]
    mount(spec, element, set_failed, attempt);
    #[cfg(not(target_arch = "wasm32"))]
    let _ = attempt;

    let empty_text = Signal::derive({
        let empty = empty.clone();
        move || {
            spec.get()
                .filter(Spec::is_empty)
                .map(|_| empty.clone())
                .unwrap_or_default()
        }
    });

    view! {
        <div class="flex flex-col gap-2">
            {caption
                .map(|caption| {
                    view! {
                        <p class="text-xs font-medium tracking-wide uppercase opacity-60">{caption}</p>
                    }
                })}
            <div class="relative h-56 w-full" node_ref=element>
                <Show when=move || spec.get().is_none()>
                    <div class="absolute inset-0 animate-pulse rounded-box bg-base-200"></div>
                </Show>
                <Show when=move || failed.get()>
                    <div class="absolute inset-0 flex flex-col items-center justify-center gap-2 rounded-box border border-base-300 bg-base-100 p-4 text-center">
                        <p class="text-sm opacity-70">
                            "Graphs are unavailable right now: the chart bundle did not load."
                        </p>
                        <button
                            class="btn btn-xs"
                            on:click=move |_| {
                                set_failed.set(false);
                                set_attempt.update(|attempt| *attempt += 1);
                            }
                        >
                            "Try again"
                        </button>
                    </div>
                </Show>
                <Show when=move || !empty_text.get().is_empty()>
                    <p class="absolute inset-0 flex items-center justify-center px-4 text-center text-sm opacity-60">
                        {move || empty_text.get()}
                    </p>
                </Show>
            </div>
        </div>
    }
}

/// Loads the bundle once, then mounts or updates the drawing in place.
#[cfg(target_arch = "wasm32")]
fn mount(
    spec: Signal<Option<Spec>>,
    element: NodeRef<leptos::html::Div>,
    set_failed: WriteSignal<bool>,
    attempt: ReadSignal<u32>,
) {
    use leptos::task::spawn_local;
    use wasm_bindgen::JsCast as _;
    
    let alive = StoredValue::new(true);
    let generation = StoredValue::new(0_u32);
    let handle = StoredValue::new_local(None::<js_sys::Object>);

    let destroy = move || {
        if let Some(mounted) = handle.get_value() {
            let _ = crate::script::call_method(&mounted, "destroy", &[]);
            handle.set_value(None);
        }
    };

    on_cleanup(move || {
        alive.set_value(false);
        destroy();
    });

    Effect::new(move |_| {
        let wanted = spec.get();
        // A retry changes the attempt, not the numbers; reading it here is what re-runs the
        // mount after `failed` is cleared.
        let _ = attempt.get();
        let run = generation.get_value().wrapping_add(1);
        generation.set_value(run);

        let Some(wanted) = wanted else {
            destroy();
            set_failed.set(false);
            return;
        };
        if wanted.is_empty() {
            destroy();
            set_failed.set(false);
            return;
        }
        let Ok(json) = serde_json::to_string(&wanted) else {
            web_sys::console::error_1(&"nodus: a chart spec could not be serialized".into());
            set_failed.set(true);
            return;
        };
        // The wrapper reads properties, not text: handing it the JSON string would leave
        // `labels` and `series` undefined and draw an empty axis.
        let Ok(object) = js_sys::JSON::parse(&json) else {
            web_sys::console::error_1(&"nodus: a chart spec could not be parsed".into());
            set_failed.set(true);
            return;
        };
        if let Some(mounted) = handle.get_value() {
            if crate::script::call_method(&mounted, "update", &[object.clone()]).is_some() {
                set_failed.set(false);
                return;
            }
            destroy();
        }

        let promise = crate::script::load_script("/vendor/charts.js", "nodusCharts");
        spawn_local(async move {
            if wasm_bindgen_futures::JsFuture::from(promise).await.is_err() {
                if alive.get_value() && generation.get_value() == run {
                    set_failed.set(true);
                }
                return;
            }
            // The app mounts into a detached tree and swaps it in, so the node can exist
            // before it is in the document; Chart.js measures its parent.
            for _ in 0..120 {
                if !alive.get_value() || generation.get_value() != run {
                    return;
                }
                if element
                    .get_untracked()
                    .is_some_and(|node| node.is_connected())
                {
                    break;
                }
                crate::script::next_frame().await;
            }
            if !alive.get_value() || generation.get_value() != run {
                return;
            }
            let Some(node) = element.get_untracked() else {
                set_failed.set(true);
                return;
            };
            let Some(charts) = crate::script::global("nodusCharts") else {
                set_failed.set(true);
                return;
            };
            let target: web_sys::Element = node.unchecked_into();
            let Some(mounted) =
                crate::script::call_method(&charts, "mount", &[target.into(), object])
            else {
                set_failed.set(true);
                return;
            };
            if let Ok(mounted) = mounted.dyn_into::<js_sys::Object>() {
                handle.set_value(Some(mounted));
                set_failed.set(false);
            } else {
                set_failed.set(true);
            }
        });
    });
}
