// A point picked in a view: what a ray met there (`{ hit, model, member, normal, … }`: parts.js `pickIn`),
// as the one thing chosen, and as what a right-click in a view is on (kinds.js).

import { f3, scanned, sel, shortPath } from "./state.js";
import { goTo } from "./links.js";
import { placed } from "./chosen.js";
import { kind } from "./kinds.js";
import { evidenceLine } from "./parts.js";
import { fitPlane } from "./points.js";
import { slabAt, surfaceAt } from "./slabs.js";
import { vec } from "../glview.js";
import { scanLook } from "../prefs.js";

/** Whether a point picked is on the member it names (not merely near it). */
const onFace = (p) => p.member && p.member.distance < 0.06;

kind("point", {
  field: "pick",
  find: (p) => p,
  title: (p) => `point ${f3(p.model)}`,
  goto: () => { if (sel.pick?.hit) goTo({ min: sel.pick.hit, max: sel.pick.hit }, 1.5); },
  // (The member it is on comes first; one it is only near, after the point itself.)
  where(p) {
    const member = p.member && { key: "member", label: `Member ${shortPath(p.member.path)}` };
    return [onFace(p) && member, p.hit && { key: "point", label: `Point ${f3(p.model)}` }, !onFace(p) && member].filter(Boolean);
  },
  // (A point of the scan: how the scan's points are drawn, under what it is near.)
  card: (p) => {
    const look = p.scan ? scanLook() : null;
    return {
      label: p.scan ? "Scan point" : "Point", id: f3(p.model),
      body: (p.member ? [`${p.member.distance ? `${(p.member.distance * 1000).toFixed(0)} mm from` : "on"} ${shortPath(p.member.path)}`, evidenceLine(p.member.path)].filter(Boolean).join(" · ") : "no member within 0.5 m") + (look ? `<br>${look.html}` : ""),
      ...(look ? { bind: look.bind } : {}),
    };
  },
  // By what is pointed at there, each with what can be done with it (what is of issues last): the member, the
  // face (of it, or of the scan) the point is on, and the point itself.
  menu(p, m, v) {
    if (p.member) m.group(`${onFace(p) ? "Member" : "Near member"} ${placed(p.member.path)}`, (g) => g.of("member", p.member.path));
    if (surfaceAt(p)) m.group("Face here", (g) => {
      g.add("Slab along it", () => slabAt("along", p, v));
      g.add("Cut through it", () => slabAt("through", p, v));
      if (scanned()) g.add("Fit a plane to it (points within 1 m)", () => fitPlane({ seed: p.hit, frame: "scan", radius: 1.0 }));
    });
    m.group(`Point ${f3(p.model)}`, (g) => {
      if (v) g.add("Orbit about it", () => v.centreOn(p.hit));
      g.head("Slab through it");
      g.add("Plan at this height", () => slabAt("plan", p, v));
      g.add("Cut across x", () => slabAt("x", p, v));
      g.add("Cut across y", () => slabAt("y", p, v));
      g.issue("New issue at this point", "point");
    });
  },
  // The point chosen (or where on the member chosen it was clicked): a small cross.
  marks(out) {
    const p = (sel.pick ?? sel.at)?.hit, r = 0.08;
    if (p) for (const a of [[r, 0, 0], [0, r, 0], [0, 0, r]]) out.line(vec.sub(p, a), vec.add(p, a), [255, 220, 40]);
  },
});
