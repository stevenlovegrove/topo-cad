// The small pictures that say what a row of the tree is of: by the kind of thing, or (a group's
// own row) by the group. One table, so a slab looks the same under "Slabs", under an issue and
// under "Unsorted".

const svg = (d) => `<svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round" stroke-linecap="round">${d}</svg>`;

const PAGE = `<path d="M4 1.8h5.2L12 4.6v9.6H4z"/><path d="M9.2 1.8v2.8H12"/>`;
const CUBE = `<path d="M8 1.8l5.4 3v6.4L8 14.2l-5.4-3V4.8z"/><path d="M2.6 4.8L8 7.8l5.4-3M8 7.8v6.4"/>`;
const PICTURES = {
  project: `<path d="M2 7.2L8 2.2l6 5M3.6 6.4v7.4h8.8V6.4"/>`,
  sheet: `${PAGE}<path d="M6 8.2h4M6 10.6h4"/>`,
  file: `${PAGE}<path d="M7 8l-1.4 1.6L7 11.2M9 8l1.4 1.6L9 11.2"/>`,
  model: CUBE,
  view: CUBE,
  group: `<path d="M1.8 4h4.4l1.4 1.6h6.6v7.4H1.8z"/>`,
  member: `<path d="M2.2 11.2l8.6-7 3 1.6-8.6 7z"/>`,
  scan: `<g fill="currentColor" stroke="none"><circle cx="3.4" cy="4" r="1"/><circle cx="8" cy="3" r="1"/><circle cx="12.4" cy="5" r="1"/><circle cx="5.4" cy="8" r="1"/><circle cx="10.4" cy="8.6" r="1"/><circle cx="3.6" cy="12" r="1"/><circle cx="8" cy="12.6" r="1"/><circle cx="12.6" cy="12" r="1"/></g>`,
  issue: `<path d="M2.2 3h11.6v7.6H7.4L4.4 13.4v-2.8H2.2z"/>`,
  selection: `<rect x="2.4" y="3.4" width="11.2" height="9.2" stroke-dasharray="2 1.8"/>`,
  plane: `<path d="M1.8 11.6L5.6 4.4h8.6l-3.8 7.2z"/>`,
  // (A slab: a thin box, seen from a corner.)
  slab: `<path d="M1.8 6.6L8 3.4l6.2 3.2v2.8L8 12.6 1.8 9.4z"/><path d="M1.8 6.6L8 9.8l6.2-3.2M8 9.8v2.8"/>`,
  polyline: `<path d="M2.6 12.4l3.4-7 4 4.6 3.4-6.4"/><g fill="currentColor" stroke="none"><circle cx="2.6" cy="12.4" r="1.3"/><circle cx="6" cy="5.4" r="1.3"/><circle cx="10" cy="10" r="1.3"/><circle cx="13.4" cy="3.6" r="1.3"/></g>`,
  // (A view kept: a camera.)
  saved: `<path d="M1.8 5h3l1.2-1.8h4L11.2 5h3v7.8H1.8z"/><circle cx="8" cy="8.8" r="2.3"/>`,
  rooms: `<path d="M2.2 2.6h11.6v10.8H2.2zM8 2.6v4.2M8 9.4v4M2.2 8h3.4"/>`,
  // (Parts nested on a sheet of stock.)
  nest: `<rect x="1.8" y="3" width="12.4" height="10"/><path d="M3.4 11.4l3-6.4 3 6.4z"/><rect x="9.8" y="5" width="2.8" height="3"/>`,
  // (A drag: a point drawn along a ray.)
  drag: `<path d="M2.2 13.8L13.8 2.2" stroke-dasharray="2 1.6"/><circle cx="8" cy="8" r="2.2"/>`,
  scratch: `<path d="M2 8.6l1.8-5h8.4l1.8 5v4.2H2zM2 8.6h3.6l.8 1.6h3.2l.8-1.6H14"/>`,
};
// A group's row has the picture of what it is a group of.
const GROUPS = { drawings: "sheet", plans: "nest", files: "file", views: "saved", issues: "issue", selections: "selection", planes: "plane", slabs: "slab", polylines: "polyline", photos: "saved", drags: "drag", participants: "person" };
PICTURES.photo = PICTURES.saved;
PICTURES.person = `<circle cx="8" cy="5.4" r="2.6"/><path d="M3 13.5c.4-3 2.4-4.6 5-4.6s4.600 1.600 5 4.600"/>`;

/** The picture for a row: of its kind, or, a group's own row ("top"), of the group its key names; a document's, of what it is (its key: `sheet:…`, `file:…`). */
export function iconOf(kind, key) {
  const name = kind === "top" ? GROUPS[key] ?? key : kind === "doc" ? key.split(":")[0] : kind;
  return PICTURES[name] ? svg(PICTURES[name]) : "";
}
