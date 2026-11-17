// Chart.js, wrapped for the console screens.
//
// The Rust side owns what the numbers are; this file owns how they are drawn and where the
// colours come from. Every colour is read from the page's custom properties, so a light and a
// dark theme are the same drawing in different ink; a `data-theme` change redraws every live
// chart rather than waiting for the next navigation.

import {
  BarController,
  BarElement,
  CategoryScale,
  Chart,
  Legend,
  LineController,
  LineElement,
  LinearScale,
  PointElement,
  Tooltip,
} from "chart.js";

Chart.register(
  BarController,
  BarElement,
  CategoryScale,
  Legend,
  LineController,
  LineElement,
  LinearScale,
  PointElement,
  Tooltip,
);

const SERIES_VARS = [
  "--color-primary",
  "--color-secondary",
  "--color-accent",
  "--color-info",
  "--color-success",
  "--color-warning",
];

const FALLBACKS = {
  "--color-primary": "#4f46e5",
  "--color-secondary": "#0ea5e9",
  "--color-accent": "#14b8a6",
  "--color-info": "#3b82f6",
  "--color-success": "#22c55e",
  "--color-warning": "#f59e0b",
  "--color-base-content": "#1f2937",
  "--color-base-300": "#d1d5db",
  "--color-base-100": "#ffffff",
};

/** @type {Set<{chart: Chart, canvas: HTMLCanvasElement, spec: object}>} */
const instances = new Set();
let watching = false;

function readVar(name) {
  const value = getComputedStyle(document.documentElement)
    .getPropertyValue(name)
    .trim();
  return value || FALLBACKS[name] || "";
}

function palette() {
  return {
    series: SERIES_VARS.map(readVar),
    content: readVar("--color-base-content"),
    grid: readVar("--color-base-300"),
    surface: readVar("--color-base-100"),
  };
}

function datasetsOf(spec, colours) {
  return (spec.series ?? []).map((series, index) => {
    const colour = colours[index % colours.length];
    const kind = series.kind ?? spec.kind ?? "line";
    if (kind === "bar") {
      return {
        type: "bar",
        label: series.label,
        data: series.values,
        backgroundColor: colour,
        borderColor: colour,
        borderWidth: 1,
        borderRadius: 3,
        maxBarThickness: 26,
      };
    }
    return {
      type: "line",
      label: series.label,
      data: series.values,
      borderColor: colour,
      backgroundColor: colour,
      borderWidth: 2,
      pointRadius: 2,
      pointHoverRadius: 4,
      tension: 0.25,
      spanGaps: true,
    };
  });
}

function build(spec, colours) {
  const datasets = datasetsOf(spec, colours);
  return {
    type: spec.kind === "bar" ? "bar" : "line",
    data: { labels: spec.labels ?? [], datasets },
    options: {
      responsive: true,
      maintainAspectRatio: false,
      animation: { duration: 180 },
      interaction: { mode: "index", intersect: false },
      plugins: {
        legend: {
          display: datasets.length > 1,
          labels: { color: colours.content, boxWidth: 10, boxHeight: 10 },
        },
        tooltip: {
          backgroundColor: colours.surface,
          titleColor: colours.content,
          bodyColor: colours.content,
          borderColor: colours.grid,
          borderWidth: 1,
        },
      },
      scales: {
        x: {
          grid: { display: false },
          border: { color: colours.grid },
          ticks: { color: colours.content, maxRotation: 0, maxTicksLimit: 8 },
        },
        y: {
          beginAtZero: spec.beginAtZero !== false,
          grid: { color: colours.grid },
          border: { display: false },
          ticks: {
            color: colours.content,
            maxTicksLimit: 5,
            precision: spec.integer ? 0 : undefined,
          },
        },
      },
    },
  };
}

function watchTheme() {
  if (watching) return;
  watching = true;
  new MutationObserver(() => {
    const colours = palette();
    for (const instance of instances) {
      instance.chart.destroy();
      instance.chart = new Chart(instance.canvas, build(instance.spec, colours));
    }
  }).observe(document.documentElement, {
    attributes: true,
    attributeFilter: ["data-theme"],
  });
}

/**
 * Draws `spec` inside `element` and returns a handle to change or remove it.
 *
 * @param {HTMLElement} element
 * @param {object} spec
 * @returns {{update: (next: object) => void, destroy: () => void}}
 */
function mount(element, spec) {
  if (!element) throw new Error("nodusCharts.mount needs an element");
  watchTheme();
  const canvas = document.createElement("canvas");
  canvas.setAttribute("role", "img");
  canvas.setAttribute("aria-label", spec.label ?? "chart");
  if (getComputedStyle(element).position === "static") {
    element.style.position = "relative";
  }
  element.appendChild(canvas);
  const instance = {
    spec,
    canvas,
    chart: new Chart(canvas, build(spec, palette())),
  };
  instances.add(instance);
  return {
    update(next) {
      instance.spec = next;
      instance.chart.destroy();
      instance.chart = new Chart(canvas, build(next, palette()));
    },
    destroy() {
      instances.delete(instance);
      instance.chart.destroy();
      canvas.remove();
    },
  };
}

globalThis.nodusCharts = { mount };
