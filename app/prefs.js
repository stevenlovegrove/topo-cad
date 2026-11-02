// How the views look, as this viewer has chosen: one struct, its defaults here, kept in this browser between visits.
// Each view takes it when it is made, and again whenever it changes. (What one view alone is, where it looks and what
// it is clipped to, is the view's.)

import { COLOURS } from "./glview.js";
import { esc, store, stored } from "./ui.js";

const KEY = "topo.view.prefs";
/** What a viewer has not chosen. */
export const PREF_DEFAULTS = Object.freeze({
  /** The scan's points: how big (pixels), what they are coloured by, and what of those the rays doubt is done with. */
  pointSize: 1, colour: "intensity", doubt: "fade",
  /** Whether a shift-drag takes every point in its window, however far behind, not only the nearest surface. */
  selectAll: false,
  /** What the model's parts are coloured by: their material ("material"), how well the scan agrees with them ("scan"),
   *  or how far the checks use them (one of the checks panel's: "utilization", "stress", …). */
  modelColour: "scan",
  /** What the views draw of the model's bodies: their faces, their edges, seen through (x-ray), and the part's nodes. */
  display: Object.freeze({ solids: true, edges: true, xray: false, nodes: false }),
  /** How the gizmo in a 3D view's corner looks: a compass or a cube (a right-click on it goes to the other), and its
   *  labels the model's compass headings ("compass": north, east, …, top, bottom) or its axes ("axes": ±X, ±Y, ±Z). */
  gizmo: Object.freeze({ style: "compass", labels: "compass" }),
});

const kept = stored(KEY, {});
let mine = { ...PREF_DEFAULTS, ...kept, display: { ...PREF_DEFAULTS.display, ...kept.display }, gizmo: { ...PREF_DEFAULTS.gizmo, ...kept.gizmo } };
const told = [];

/** The viewer's choices, as they are. */
export const prefs = () => ({ ...mine });
/** Changes some of them: kept, and every view told. */
export function setPrefs(patch) {
  mine = { ...mine, ...patch };
  store(KEY, mine);
  for (const f of told) f(patch);
}
/** Is told of each change (what changed). */
export const onPrefs = (f) => told.push(f);

/** What the scan looks like, as a card shows it to be changed: its points' colour, their size, and doubtful ones. */
export function scanLook() {
  const p = mine;
  const html =
    `<span class="look"><label title="What the scan's points are coloured by (depth: across the slab a view is clipped to)">Colour <select data-look="colour">${COLOURS.map((c) => `<option ${c === p.colour ? "selected" : ""}>${esc(c)}</option>`).join("")}</select></label>` +
    ` <label title="How big the scan's points are drawn (pixels)">Size <input type="range" min="1" max="6" step="0.5" value="${p.pointSize}" data-look="pointSize"></label>` +
    ` <label title="Points the scan's rays or the lidar's flags doubt (other rays pass where they are: smears behind edges, stray returns), in a view clipped to a slab. Colour by confidence to see them.">Doubtful <select data-look="doubt">${["show", "fade", "hide"].map((d) => `<option value="${d}" ${d === p.doubt ? "selected" : ""}>${d === "show" ? "shown" : d === "fade" ? "faded" : "hidden"}</option>`).join("")}</select></label></span>`;
  const bind = (card) => {
    for (const i of card.querySelectorAll("[data-look]")) {
      const k = i.dataset.look, read = () => (k === "pointSize" ? Number(i.value) : i.value);
      i.onkeydown = (e) => e.stopPropagation();
      (k === "pointSize" ? (h) => (i.oninput = h) : (h) => (i.onchange = h))(() => setPrefs({ [k]: read() }));
    }
  };
  return { html, bind };
}
