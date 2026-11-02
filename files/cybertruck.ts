// A scale model of a Cybertruck (examples/cybertruck.ts), written on the part core: about 200 nose to tail (about 1:28),
// cut flat of 2 mm sheet of 304 stainless and welded: nothing is bent. The angles are in how the plates' edges are cut.
//
// (Ported from the present pipeline's: old-pipeline, examples/cybertruck.ts.)
//
// Its proportions are the truck's, fitted to its edges in two photos (`cybertruck.fit.ts`, what is seen in them in
// `cybertruck.sightings.ts`): the body's lower edge, the overhangs, the arches, the beltline, the roof's peak and its
// line, the hood, the light bar and the rocker's lower edge. The truck is made at what the fit finds. Its size is the
// truck's wheelbase, which no photo can say.
//
// The body is fifteen plates, meeting corner to corner, none standing past another, and each seam is fingered: fingers
// of the two plates by turns put each where it goes before it is tacked, and it is TIG welded.
//   - A side each way, upright: from the tail along the beltline (which falls a little toward the front), down the
//     hood's edge to the front corner, down it to the body's lower edge, and back along it, with an arch cut over each
//     wheel.
//   - A window each way above it, leaning in: its lower edge along the beltline, its point at the peak of the roof, its
//     edge along the roof's line the roof's as far back as the cab's back.
//   - The hood, from the light bar up to the peak; the roof, from the peak down to the cab's back: each as wide as the
//     sides at its low end, and narrower at the peak by what the windows lean in.
//   - The front: the light bar's band, a short upright under the hood's nose, narrower than the body; under it the
//     chin, sloping back to the body's lower edge as wide as the body; and at each corner, a chamfer, upright, at 45
//     degrees between the band and the chin and the side. Across the back, under the tailgate, the sill.
//   - The floor, level, from the sill to the chin, as high as the bed's floor is: standing in the sides, the chin and
//     the sill by tabs through slots in them, cut out under each tub.
//   - Over each wheel, a tub that shuts its arch off from inside the body over the floor: a wall parallel to the side,
//     10 in from it, standing on the floor, and three plates from it to the side's edges round the arch (its top, and
//     its two slopes), butted to the side's inner face.
//   - The cab's back, upright across the truck between the roof's back edge and the floor.
//
// Each plate is drawn on its inner face (its underside), its corners where they meet the others free: the seams put
// them, inner corner on inner corner. x is forward, y to the left, z up; the ground under the tailgate is the origin.

import { part, node, free, when, as, placed, joints, extrusion, revolve, polygon, sweep, plane, line, cut, endOn, sheet, iso, plan, front, back, side, laidFlat, schedule, materials, customMaterial, GPa, MPa, kgm3, mm, deg, type Length, type Corner } from "topo-part";
import { plate, onFace, atCorner, cutOut, welds } from "topo-part/sheet";
import { fit, held, type Proportions } from "./cybertruck.fit";

type V = [number, number, number];
const sub = (a: V, b: V): V => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const dot = (a: V, b: V) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const cross = (a: V, b: V): V => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
const unit = (a: V): V => { const n = Math.hypot(...a); return [a[0] / n, a[1] / n, a[2] / n]; };
const sides: [string, number][] = [["L", 1], ["R", -1]];

// ----- the stock -----------------------------------------------------------------------------------------------------

// 304 stainless, 2 mm. (Annealed sheet: ASTM A240's least yield and tensile; its stiffness and density the handbooks'
// usual. Nothing here is checked for strength: it is a model.)
const t = held.t;
const s304 = customMaterial({ name: "304 stainless", family: "steel", e: GPa(193), g: GPa(77), density: kgm3(8000), strength: MPa(205), ultimate: MPa(515), source: "ASTM A240 least yield and tensile strength of annealed 304 sheet; E, G and density typical handbook values, from memory, not looked up" });

// The truck: once its proportions are found (fitted to the photos: `cybertruck.fit.ts`), made at them, in a scope of
// the root's; the fit (its proportions, its photos, the edges seen in them) the root's own.
export default part("cybertruck", (root) => {
  const found = fit();
  when(root, (s) => ({ truck: truck(Object.fromEntries(Object.entries(found.proportions).map(([k, p]) => [k, as(s.value(p), mm)])) as Proportions) }));
  return { ...found.proportions, nodes: found.nodes, edges: found.edges, ...found.photos, says: found.says };
});

/** The truck, at its proportions. */
function truck(fit: Proportions) {
  // ----- what is where (the inner faces) -------------------------------------------------------------------------------

  // Its proportions, as the fit found them; and what is held (`held`): its size is the truck's wheelbase, 3807 (134 at
  // this scale), which no photo can say; and its width, 2.03 m.
  const w = held.w; // the sides' inner faces, either way
  const zb = fit.bodyLow; // the body's lower edge, all round
  const floorZ = 33; // the floor's top, which is the bed's
  const tail = { x: 0, z: fit.tailTop }; // the sails' points: where the beltline and the roof begin
  const peak = { x: fit.peakAlong, z: fit.peakHigh, w: held.peakW }; // the roof's peak: how far forward, how high, how wide
  const chamfer = fit.chamfer;
  const bar = { x: fit.barAlong, z: fit.barHigh, w: w - chamfer }; // the light bar's top edge
  const corner = bar.x - (w - bar.w);
  const chinTop = bar.z - held.band;
  const frontLow = held.frontLow; // where the stainless ends at the front corners, over the front wheels
  const beltFrom = corner - fit.beltBehind; // where the beltline meets the hood's edge, behind the front corner
  /** The hood's inner face (and the windscreen's), so far forward: from the peak to the light bar. */
  const hoodZ = (x: number) => peak.z + ((bar.z - peak.z) * (x - peak.x)) / (bar.x - peak.x);
  /** The chin's inner face, so far up: from the light bar's band down to the body's lower edge at the corners. */
  const chinX = (z: number) => corner + ((bar.x - corner) * (z - zb)) / (chinTop - zb);
  const wheelbase = held.wheelbase;
  const axles = { rear: fit.rearAxle, front: fit.rearAxle + wheelbase };
  const arch = { top: fit.archTop, foot: fit.archFoot, rear: fit.rearArch, front: fit.frontArch };
  const frontArch = { gap: fit.frontArchGap, high: fit.frontArchHigh, meets: held.meets };
  const arches: Record<keyof typeof axles, { r: number; rt: [number, number]; ft: [number, number]; f: number }> = {
    rear: { r: axles.rear - arch.foot, rt: [axles.rear - arch.top, arch.rear], ft: [axles.rear + arch.top, arch.rear], f: axles.rear + arch.foot },
    front: { r: axles.front - arch.foot, rt: [axles.front - arch.top, arch.front], ft: [corner - frontArch.gap, frontArch.high], f: corner },
  };
  const cabX = held.cabX, cabRise = fit.cabRise;
  const sill = floorZ + 2.5;
  // The tailgate: where it turns (along the truck and up: as high over the sill as a gate's thickness and the ears' reach
  // under the pin, so that it lies on the sill when it is down), the clearance round it, and its pins' holes.
  const gateClear = 0.3, gateLift = t + 0.1, pinHole = 2.2;
  const pivot: V = [3, 0, sill + 5];

  // The corners where the inner faces meet, each way (s: 1 left, −1 right).
  const at = {
    tail: (s: number): V => [tail.x, s * w, tail.z],
    meets: (s: number): V => [corner, s * w, frontArch.meets],
    front: (s: number): V => [corner, s * w, hoodZ(corner)],
    low: (s: number): V => [corner, s * w, frontLow],
    chinLow: (s: number): V => { const x = chinX(frontLow); return [x, s * (w - (x - corner)), frontLow]; },
    bar: (s: number): V => [bar.x, s * bar.w, bar.z],
    chin: (s: number): V => [bar.x, s * bar.w, chinTop],
    rear: (s: number): V => [tail.x, s * w, zb],
    peak: (s: number): V => [peak.x, s * peak.w, peak.z],
    belt: (s: number): V => [beltFrom, s * w, hoodZ(beltFrom)],
    // (The roof's back edge, at the cab's back, so high over the straight line from the tail to the peak: in the
    // window's plane.)
    cab: (s: number): V => {
      const a = at.tail(s), n = cross(sub(at.belt(s), a), sub(at.peak(s), a)), z = tail.z + ((peak.z - tail.z) * (cabX - tail.x)) / (peak.x - tail.x) + cabRise;
      return [cabX, a[1] - (n[0] * (cabX - a[0]) + n[2] * (z - a[2])) / n[1], z];
    },
    cabBelt: (s: number): V => { const k = (cabX - tail.x) / (beltFrom - tail.x), a = at.tail(s), b = at.belt(s); return [a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k, a[2] + (b[2] - a[2]) * k]; },
  };

  /** A corner of a plate where it is in the truck: free (put by its seams, drawn only about there) or not. */
  type C = { name: string; p: V; free?: boolean };
  const free_ = (name: string, p: V): C => ({ name, p, free: true });

  /** Each plate's own frame, as a point of the truck is in it (millimetres, its x and y): for what is cut in it. */
  const localOf = new Map<object, (p: V) => [number, number]>();
  /** Each plate's way out of the truck. */
  const outOf = new Map<object, V>();
  /** Each plate's frame in the truck: its origin and its axes. */
  const framesOf = new Map<object, { o: V; ex: V; ey: V }>();

  /** A plate on its inner face: its corners where they are in the truck, which way it looks out, and which way its own x
   *  is: its frame at its first corner, its corners in it (a free one free in its plane). */
  function shell(name: string, corners: C[], out: V, xdir: V, more: { holes?: { name: string; p: V; d: number }[]; inFrameAt?: V } = {}) {
    const n = unit(out), o = corners[0].p;
    const ex = unit(sub(xdir, [n[0] * dot(xdir, n), n[1] * dot(xdir, n), n[2] * dot(xdir, n)])), ey = cross(n, ex);
    const xy = (p: V): [number, number] => { const d = sub(p, o); return [dot(d, ex), dot(d, ey)]; };
    // (Anticlockwise seen from its top, out: said the other way round where they go round the other way.)
    const pts = corners.map((c) => xy(c.p));
    const area = pts.reduce((a, p, k) => a + p[0] * pts[(k + 1) % pts.length][1] - pts[(k + 1) % pts.length][0] * p[1], 0);
    const outline = (area < 0 ? [...corners].reverse() : corners).map(({ name: c, p, free: f }) => {
      const [x, y] = xy(p);
      return [c, f ? [free(mm(x)), free(mm(y))] : [mm(x), mm(y)]] as [string, [Length, Length]];
    });
    // (Round holes, each where it is in the truck; and, in a frame whose origin is a point of the truck (its axes the
    // truck's), placed from there.)
    const holes = Object.fromEntries((more.holes ?? []).map((h) => { const [x, y] = xy(h.p); return [h.name, { at: [mm(x), mm(y)] as [Length, Length], d: mm(h.d) }]; }));
    const from = more.inFrameAt ? sub(o, more.inFrameAt) : o;
    const p = plate(name, outline, mm(t), s304, { at: placed({ origin: [mm(from[0]), mm(from[1]), mm(from[2])], x: ex, y: ey }), holes });
    localOf.set(p, xy);
    outOf.set(p, n);
    framesOf.set(p, { o, ex, ey });
    return p;
  }

  // The glass is cut out of the plates it is in, a margin in from their edges as designed (which is past what the fingers
  // of their seams cut back), its corners rounded: what is left round it is the pillars.
  const glass = { margin: 3.5, round: 1.5, pillar: { from: 111, to: 116 }, screen: 172 };
  /** A pane of a plate: a shape in the truck, seen square onto the plate, or the whole of it between two places along the
   *  truck (`[x0, x1]`: in the plate's plane, a long way across it either way). In the plate's own frame. */
  function pane(p: object, q: V[] | [number, number]): [Length, Length][] {
    const n = outOf.get(p)!, xy = localOf.get(p)!, origin = framesOf.get(p)!.o;
    const across = unit(cross(n, [1, 0, 0])), along = cross(across, n);
    // (A point of the plate's plane so far along the truck and so far across, from its frame's origin, which is in it.)
    const on = (x: number, k: number): V => {
      const u = (x - origin[0]) / along[0];
      return [origin[0] + along[0] * u + across[0] * k, origin[1] + along[1] * u + across[1] * k, origin[2] + along[2] * u + across[2] * k];
    };
    const shape: V[] = typeof q[0] === "number" ? [on(q[0] as number, -300), on(q[1] as number, -300), on(q[1] as number, 300), on(q[0] as number, 300)] : (q as V[]);
    return shape.map((v) => xy(v)).map(([x, y]) => [mm(x), mm(y)]);
  }

  // ----- the body ------------------------------------------------------------------------------------------------------

  /** Where an arch's slope (from its top corner down to its foot on the body's lower edge) is as high as the floor. */
  const onFloor = (top: [number, number], foot: number) => top[0] + ((foot - top[0]) * (top[1] - floorZ)) / (top[1] - zb);

  // A side: upright, its outline round from the tail along the beltline, down the hood's edge to the front corner, down
  // it to the body's lower edge, and back along it with an arch over each wheel, and up its back edge, past the sill, to
  // the tail.
  const sidePlates = sides.map(([side, s]) => {
    const y = s * w;
    const archOf = (k: keyof typeof axles): C[] => {
      const a = arches[k];
      return [...(k === "front" ? [] : [{ name: `${k} arch f`, p: [a.f, y, zb] as V }]), { name: `${k} arch ft`, p: [a.ft[0], y, a.ft[1]] }, { name: `${k} arch rt`, p: [a.rt[0], y, a.rt[1]] }, { name: `${k} arch r`, p: [a.r, y, zb] }];
    };
    const corners: C[] = [{ name: "tail", p: at.tail(s) }, free_("belt", at.belt(s)), free_("front", at.front(s)), { name: "front arch meets", p: at.meets(s) }, ...archOf("front"), ...archOf("rear"), { name: "rear", p: at.rear(s) }, { name: "sill", p: [tail.x, y, sill] }];
    // (A hole for the tailgate's pin.)
    return shell(`side ${side}`, corners, [0, s, 0], [-s, 0, 0], { holes: [{ name: "pin", p: [pivot[0], y, pivot[2]], d: pinHole }] });
  });

  // A window: leaning in from the beltline to the peak. (Its plane is the beltline's and the peak's.)
  const windows = sides.map(([side, s]) => {
    const n = cross(sub(at.belt(s), at.tail(s)), sub(at.peak(s), at.tail(s)));
    const corners = [free_("tail", at.tail(s)), free_("belt", at.belt(s)), free_("peak", at.peak(s)), free_("cab", at.cab(s))];
    return shell(`window ${side}`, corners, n[1] * s > 0 ? n : [-n[0], -n[1], -n[2]], [1, 0, 0]);
  });

  /** Which way a plate looks that is across the truck and goes so far along it as it rises so far. */
  const across = (dx: number, dz: number): V => [-dz, 0, dx];
  const hood = shell("hood", [free_("bar R", at.bar(-1)), free_("bar L", at.bar(1)), free_("side L", at.front(1)), free_("belt L", at.belt(1)), free_("peak L", at.peak(1)), free_("peak R", at.peak(-1)), free_("belt R", at.belt(-1)), free_("side R", at.front(-1))], across(bar.x - peak.x, bar.z - peak.z), [0, 1, 0]);
  const roof = shell("roof", [free_("peak R", at.peak(-1)), free_("peak L", at.peak(1)), free_("back L", at.cab(1)), free_("back R", at.cab(-1))], across(peak.x - cabX, peak.z - at.cab(1)[2]), [0, 1, 0]);
  const band = shell("light bar", [free_("top R", at.bar(-1)), free_("top L", at.bar(1)), free_("low L", at.chin(1)), free_("low R", at.chin(-1))], [1, 0, 0], [0, 1, 0]);
  const chin = shell("chin", [{ name: "bottom R", p: at.chinLow(-1) }, { name: "bottom L", p: at.chinLow(1) }, free_("top L", at.chin(1)), free_("top R", at.chin(-1))], across(bar.x - corner, chinTop - zb).map((v) => -v) as V, [0, 1, 0]);
  const chamfers = sides.map(([side, s]) => {
    const corners: C[] = [{ name: "rear low", p: at.low(s) }, { name: "meets", p: at.meets(s) }, free_("rear top", at.front(s)), free_("front top", at.bar(s)), free_("front low", at.chin(s)), { name: "low", p: at.chinLow(s) }];
    return shell(`chamfer ${side}`, corners, [1, s, 0], [-s, 1, 0]);
  });
  const sillPlate = shell("sill", [{ name: "rear L", p: at.rear(1) }, { name: "rear R", p: at.rear(-1) }, { name: "sill R", p: [tail.x, -w, sill] }, { name: "sill L", p: [tail.x, w, sill] }], [-1, 0, 0], [0, -1, 0]);

  // The floor: level, from the sill to the chin, between the sides and the chamfers; under each tub cut out, a notch in
  // its edge along the side between where the tub's slopes and its wall stand on it, a lip in from each.
  const floorChin = chinX(floorZ - t), floorAt = { rear: (s: number): V => [tail.x, s * w, floorZ], side: (s: number): V => [corner, s * w, floorZ], chin: (s: number): V => [floorChin, s * (w - (floorChin - corner)), floorZ] };
  const tubIn = 10, lip = 1;
  const notch = (k: keyof typeof axles, side: string, s: number): C[] => {
    const a = arches[k], [r, f] = [onFloor(a.rt, a.r) + lip, onFloor(a.ft, a.f) - lip], o = s * w, i = s * (w - tubIn + lip);
    const c: C[] = [{ name: `${k} tub ${side} a`, p: [r, o, floorZ] }, { name: `${k} tub ${side} b`, p: [r, i, floorZ] }, { name: `${k} tub ${side} c`, p: [f, i, floorZ] }, { name: `${k} tub ${side} d`, p: [f, o, floorZ] }];
    return s < 0 ? c : c.reverse();
  };
  const floor = shell("floor", [free_("rear R", floorAt.rear(-1)), ...notch("rear", "R", -1), ...notch("front", "R", -1), free_("side R", floorAt.side(-1)), free_("chin R", floorAt.chin(-1)), free_("chin L", floorAt.chin(1)), free_("side L", floorAt.side(1)), ...notch("front", "L", 1), ...notch("rear", "L", 1), free_("rear L", floorAt.rear(1))], [0, 0, -1], [1, 0, 0]);
  // The cab's back: upright across the truck, from the floor to the roof's back edge, between the sides and, over the
  // beltline, the windows.
  const cabBack = shell("cab back", [free_("low R", [cabX, -w, floorZ]), free_("low L", [cabX, w, floorZ]), { name: "belt L", p: at.cabBelt(1) }, free_("top L", at.cab(1)), free_("top R", at.cab(-1)), { name: "belt R", p: at.cabBelt(-1) }], [-1, 0, 0], [0, 1, 0]);

  // A tub over each wheel's arch: along each of the arch's three edges in the side, a plate in to a wall parallel to the
  // side, 10 in, that stands on the floor. Its plates are drawn on their faces toward the space over the wheel, flush with
  // the arch's edges, so that they meet one another corner to corner.
  const tubs = (Object.keys(axles) as (keyof typeof axles)[]).flatMap((k) => sides.map(([side, s]) => {
    const a = arches[k], o = s * w, i = s * (w - tubIn), lo = floorZ;
    const [f, r] = [onFloor(a.ft, a.f), onFloor(a.rt, a.r)], [[ft, fh], [rt, rh]] = [a.ft, a.rt];
    const name = `tub ${k} ${side}`;
    const way = (c: C[]) => c;
    return {
      k, side, s,
      long: { front: Math.hypot(f - ft, fh - lo), rear: Math.hypot(rt - r, rh - lo) },
      top: shell(`${name} top`, way([free_("ft", [ft, o, fh]), free_("rt", [rt, o, rh]), free_("rti", [rt, i, rh]), free_("fti", [ft, i, fh])]), [rh - fh, 0, ft - rt], [1, 0, 0]),
      front: shell(`${name} front`, way([free_("ft", [ft, o, fh]), free_("ff", [f, o, lo]), free_("ffi", [f, i, lo]), free_("fti", [ft, i, fh])]), [fh - lo, 0, f - ft], [0, 1, 0]),
      rear: shell(`${name} rear`, way([free_("rt", [rt, o, rh]), free_("rf", [r, o, lo]), free_("rfi", [r, i, lo]), free_("rti", [rt, i, rh])]), [-(rh - lo), 0, rt - r], [0, 1, 0]),
      wall: shell(`${name} wall`, way([free_("rf", [r, i, lo]), free_("ff", [f, i, lo]), free_("ft", [ft, i, fh]), free_("rt", [rt, i, rh])]), [0, -s, 0], [1, 0, 0]),
    };
  }));

  // ----- the tailgate --------------------------------------------------------------------------------------------------

  // The tailgate turns down: it is on two pins at the floor, each through an ear welded inside the gate's end and a hole in
  // the side. Shut, it stands on the sill between the sides' back edges, its top between the sails' points, and the ears'
  // lower edges are on the floor, which stops it; open, it lies on the sill. It turns on a line across the truck through
  // its pins (as high over the sill as a gate's thickness and the ears' reach under the pin, so that it lies on the sill
  // when it is down): a hinge, its frame's axes the truck's when the gate is shut, about −y (the way it opens), from shut to
  // down, 90 degrees. What it first meets on the way, if anything, is said.
  const gateY = w - gateClear;
  const gate = shell("tailgate", [{ name: "low L", p: [tail.x, gateY, sill + gateLift] }, { name: "low R", p: [tail.x, -gateY, sill + gateLift] }, { name: "top R", p: [tail.x, -gateY, tail.z] }, { name: "top L", p: [tail.x, gateY, tail.z] }], [-1, 0, 0], [0, -1, 0], { inFrameAt: pivot });
  // An ear each side, inside the gate at its end, upright along the truck: its back edge on the gate (butted), the pin's
  // hole in it, its lower edge on the floor in front of the pin when the gate is shut (which is what stops it there), its
  // top corner cut off. Behind the pin its lower edge rises, inside the circle the pin's centre makes to the floor, so
  // that as the gate opens it goes over the floor, not into it. Drawn on its face toward the side.
  const ear = { reach: pivot[0] + 2.5, high: pivot[2] + 3.5, under: pivot[0] + 0.5, back: pivot[2] - Math.sqrt((pivot[2] - floorZ) ** 2 - (pivot[0] - tail.x) ** 2) + 0.2 };
  const ears = sides.map(([side, s]) => shell(`tailgate ear ${side}`, [
    { name: "back low", p: [tail.x, s * gateY, ear.back] }, { name: "under pin", p: [ear.under, s * gateY, floorZ] }, { name: "front low", p: [ear.reach, s * gateY, floorZ] }, { name: "front top", p: [ear.reach, s * gateY, ear.high] }, { name: "back top", p: [tail.x, s * gateY, ear.high + 2.5] },
  ], [0, -s, 0], [1, 0, 0], { holes: [{ name: "pin", p: [pivot[0], s * gateY, pivot[2]], d: pinHole }], inFrameAt: pivot }));
  // A pin each side: a rod of 304, 2 across, through the ear and the side, its outer end flush with the side's outer face.
  // (Held by a drop of weld on its inner end: not drawn.) Along y, from inside the ear to the side's outer face.
  const pinLong = t + gateClear + t;
  const pins = sides.map(([side, s]) => {
    const centre = node(0, 0, 0), rim = node(mm(1), 0, 0);
    const at: V = [pivot[0], s * (w + t - pinLong / 2), pivot[2]];
    return part(`tailgate pin ${side}`, { centre, rim }, {
      at: placed({ origin: [mm(at[0]), mm(at[1]), mm(at[2])], x: [1, 0, 0], y: [0, 0, 1] }),
      kind: "rod",
      shapes: [extrusion([{ node: rim, arc: { centre, ccw: true } }], { from: mm(-pinLong / 2), to: mm(pinLong / 2), material: s304 })],
    });
  });

  // ----- printed: the black under the stainless, and the wheels -------------------------------------------------------

  // What is black on the truck under its stainless is printed, in black PETG: the underbody, the rockers under the doors,
  // the quarters behind the rear wheels, each well's liner and lip, a flare round each arch, and the wheels.
  const petg = materials.PETG;
  // The wheels: printed, each pressed onto its axle (a bore of 2.9 on a 3 mm rod), 9 wide, its outer face just proud of
  // the side's outer face, under its arch. A tyre (the truck's 35 in, 15.65 at this scale) with its shoulders rounded and
  // its outer face dished. Each axle turns in the underbody: a frame along it, turning about it (its z, across the truck):
  // what the wheels turn to is free, nothing here says it.
  const wheel = { r: held.wheelR, wide: 9, out: held.wheelOut, shoulder: 1, bore: 2.9, dish: 40 };
  const axleZ = wheel.r; // on the ground
  const wheelIn = wheel.out - wheel.wide; // each wheel's inner face, either way
  /** The tyre's profile (out from the axle, and along it from its inner face), its corners nodes of `p`'s; a millimetre
   *  larger all round (`out`), and in to the axle, it is the well it turns in. */
  function tyre(name: string, out: number, o: { tool?: boolean; s: number }) {
    const rim = wheel.r - 4, hub = out > 0 ? 0 : wheel.bore / 2;
    // (The dish: an arc of 40 from the rim to the hub, curving in: its centre out beyond the face, over the chord's middle.)
    const half = (rim - hub) / 2, z = wheel.wide + out;
    const nodes = {
      hubIn: node(mm(hub), 0, mm(-out)), shoulderIn: node(mm(wheel.r + out), 0, mm(-out)), shoulderOut: node(mm(wheel.r + out), 0, mm(z)),
      rim: node(mm(rim), 0, mm(z)), dish: node(mm(hub + half), 0, mm(z + Math.sqrt(wheel.dish ** 2 - half ** 2))), hubOut: node(mm(hub), 0, mm(z)),
    };
    const round = mm(wheel.shoulder * 2 + out);
    const corners: Corner[] = [{ node: nodes.hubIn }, { node: nodes.shoulderIn, round }, { node: nodes.shoulderOut, round }, ...(out > 0 ? [] : [{ node: nodes.rim, arc: { centre: nodes.dish, ccw: false } }]), { node: nodes.hubOut }];
    // (In the axle's frame, its z along the axle outward either side.)
    const at = placed({ origin: [0, 0, mm(o.s * wheelIn)], x: [1, 0, 0], y: [0, o.s, 0] });
    const p = part(name, nodes, { at, kind: "solid" });
    p.shapes = [revolve(corners, petg, { tool: o.tool })];
    return p;
  }
  // (Its bore in the underbody: 3.3, the rod turning in it.)
  const axleRod = { d: 3, long: 2 * (wheel.out + 1), bore: 3.3 };
  const axleLines: Partial<Record<keyof typeof axles, ReturnType<typeof line>>> = {};
  const axleParts = (Object.keys(axles) as (keyof typeof axles)[]).map((k) => {
    const centre = node(0, 0, 0), rim = node(mm(axleRod.d / 2), 0, 0), along = node(0, 0, mm(1));
    axleLines[k] = line(centre, along);
    const wheels = Object.fromEntries(sides.map(([side, s]) => [`wheel ${side}`, tyre(`wheel ${side}`, 0, { s })]));
    const wells = Object.fromEntries(sides.map(([side, s]) => [`well ${side}`, tyre(`well ${side}`, 1, { s, tool: true })]));
    // (The rod: 3 across, from one wheel's outer face to the other's and 1 past each, along the axle.)
    const rod = part("rod", { centre, rim, along, axis: axleLines[k] }, { kind: "rod", shapes: [extrusion([{ node: rim, arc: { centre, ccw: true } }], { from: mm(-axleRod.long / 2), to: mm(axleRod.long / 2), material: s304 })] });
    return part(`axle ${k}`, { ...wheels, ...wells, rod }, {
      at: placed({ origin: [mm(axles[k]), 0, mm(axleZ)], x: [1, 0, 0], y: [0, 0, -1], joint: joints.hinge([0, 0, 1], free(deg(0))) }),
    });
  });
  // The underbody: across the truck between the sides' inner faces, under the floor, from the sill to the chin: its top on
  // the floor's underside, its back the sill's plane (and on down under it: the rear bumper), its front the chin's plane
  // (and on down its slope: the front bumper), down far enough to hold the axles. An extrusion of its side's profile;
  // less what is outside each front corner's chamfer (so it wraps the corner), less a well for each wheel, and bored for
  // each axle.
  // (Its underside 1.6 below each axle's bore.)
  const underZ = axleZ - axleRod.bore / 2 - 1.6;
  const underProfile = { backLow: node(0, mm(underZ), 0), frontLow: node(mm(chinX(underZ)), mm(underZ), 0), frontTop: node(mm(chinX(floorZ - t)), mm(floorZ - t), 0), backTop: node(0, mm(floorZ - t), 0) };
  const underbody = part("underbody", underProfile, {
    // (Its x the truck's, its y up, so its z across the truck, from the left side's inner face to the right's.)
    at: placed({ origin: [0, mm(w), 0], x: [1, 0, 0], y: [0, 0, 1] }),
    kind: "solid",
    shapes: [extrusion(polygon(underProfile.backLow, underProfile.frontLow, underProfile.frontTop, underProfile.backTop), { to: mm(2 * w), material: petg })],
  });

  // A rocker each side, under the door between the arches; and a quarter behind each rear wheel, from the tail to the arch.
  // Each is the one profile, under the side's thickness (from its inner face out to its outer face; from the body's lower
  // edge down to the rocker's lower edge, its foot a facet in), drawn along the truck: its ends in the arches' slopes (on
  // the tubs' plates' faces toward the wheel, as the truck's black arches go), the quarter's back on the sill's inner face.
  const rockerZ = fit.rockerLow;
  function lower(name: string, s: number, from: number, to: number) {
    const at = (y: number, z: number) => node(mm(s * y), mm(z), 0);
    const nodes = { sideLow: at(w, zb), bodyEdge: at(w + t, zb), crease: at(w + t, zb - 0.69 * (zb - rockerZ)), foot: at(w + t - 1, rockerZ), inLow: at(w, rockerZ) };
    const ring = [nodes.sideLow, nodes.bodyEdge, nodes.crease, nodes.foot, nodes.inLow];
    // (Its x the truck's y, its y up: so it goes along the truck's x.)
    return part(name, nodes, { at: placed({ origin: [mm(from), 0, 0], x: [0, 1, 0], y: [0, 0, 1] }), kind: "solid", shapes: [extrusion(polygon(...(s > 0 ? ring : [...ring].reverse())), { to: mm(to - from), material: petg })] });
  }
  const rockers = sides.map(([side, s]) => lower(`rocker ${side}`, s, arches.rear.f, arches.front.r));
  const quarters = sides.map(([side, s]) => lower(`quarter ${side}`, s, tail.x, arches.rear.r));

  // Each wheel's well is lined in black, as the truck's are: inside the tub, a band 1.5 thick round under its top and its
  // slopes, from the floor up, across from the tub's wall to the side's inner face; and in the side's own thickness,
  // round the arch's edge down to where the side ends, a lip as thick (which the flare over it covers).
  const liner = 1.5;
  /** A chain of points round an arch (from its rear foot up and over to its front one, in x and z), and the same chain so
   *  far into the arch (each run moved square to itself into the opening, to the right of the way the chain goes; its ends
   *  where the moved runs at either end are as high as the chain's ends). */
  function inside(chain: [number, number][], d: number): [number, number][] {
    const n = chain.length;
    const moved = (k: number) => { const [a, b] = [chain[k], chain[k + 1]], l = Math.hypot(b[0] - a[0], b[1] - a[1]), u: [number, number] = [(b[0] - a[0]) / l, (b[1] - a[1]) / l]; return { p: [a[0] + u[1] * d, a[1] - u[0] * d] as [number, number], u }; };
    const meet = (r: ReturnType<typeof moved>, q: ReturnType<typeof moved>): [number, number] => { const det = r.u[0] * -q.u[1] + r.u[1] * q.u[0], k = ((q.p[0] - r.p[0]) * -q.u[1] + (q.p[1] - r.p[1]) * q.u[0]) / det; return [r.p[0] + r.u[0] * k, r.p[1] + r.u[1] * k]; };
    const atZ = (r: ReturnType<typeof moved>, z: number): [number, number] => [r.p[0] + (r.u[0] * (z - r.p[1])) / r.u[1], z];
    return [atZ(moved(0), chain[0][1]), ...Array.from({ length: n - 2 }, (_, k) => meet(moved(k), moved(k + 1))), atZ(moved(n - 2), chain[n - 1][1])];
  }
  /** A band round an arch: its chain and the chain moved in, extruded across the truck from `y0` toward the middle by
   *  `deep` (its x the truck's, its y up). */
  function archBand(name: string, chain: [number, number][], y0: number, deep: number) {
    const pts = [...chain, ...inside(chain, liner).reverse()];
    const nodes = Object.fromEntries(pts.map((p, k) => [`c${k}`, node(mm(p[0]), mm(p[1]), 0)]));
    return part(name, nodes, { at: placed({ origin: [0, mm(y0), 0], x: [1, 0, 0], y: [0, 0, 1] }), kind: "solid", shapes: [extrusion(polygon(...Object.values(nodes)), { to: mm(deep), material: petg })] });
  }
  const archKeys = Object.keys(axles) as (keyof typeof axles)[];
  const liners = archKeys.flatMap((k) => sides.map(([side, s]) => {
    const a = arches[k];
    // (From the floor at the rear slope, up and over, down to the floor at the front: across from the side's inner face to
    // the tub's wall, 10 in.)
    return archBand(`liner ${k} ${side}`, [[onFloor(a.rt, a.r), floorZ], a.rt, a.ft, [onFloor(a.ft, a.f), floorZ]], s > 0 ? w : -(w - tubIn), tubIn);
  }));
  const lips = archKeys.flatMap((k) => sides.map(([side, s]) => {
    const a = arches[k];
    // (Round the side's own arch edge: the front one's ends where the side does, at the front corner.)
    return archBand(`lip ${k} ${side}`, [[a.r, zb], a.rt, a.ft, k === "front" ? [corner, frontArch.meets] : [a.f, zb]], s > 0 ? w + t : -w, t);
  }));
  // A flare round each arch, as the truck's black arches are: a profile carried along the arch's edge on the side's outer
  // face (a sweep, mitred at the arch's corners), 2.5 out onto the side and 1.2 proud of it, its outer edge chamfered, and
  // over the lip by its thickness. Its ends level where the side ends at the body's lower edge, the front one's front end
  // square with the front corner. Its path is the side's arch corners on its outer face (which are where they are said).
  const flare = { onto: 2.5, proud: 1.2 };
  const flares = archKeys.flatMap((k) => sides.map(([side, s]) => {
    const a = arches[k], y = s * (w + t);
    // (Its frame's y is out of the side's face, either side: so on the right its y and its z are the truck's turned over;
    // a point of the truck (X, Y, Z) is (X, sY, sZ) in it.)
    const off = (p: [number, number, number]) => node(mm(p[0]), mm(s * p[1]), mm(s * p[2]));
    const path = [[a.r, zb], a.rt, a.ft, k === "front" ? [corner, frontArch.meets] : [a.f, zb]].map(([x, z]) => off([x, y, z]));
    // (Its outline as the sweep carries it: its x its part's y (out of the side's face) made square to the path, its y
    // across the path: out from the arch's opening one way round, into it the other.)
    const at = (out: number, up: number) => node(mm(up), mm(s * out), 0);
    const outline = { in: at(-liner, 0), out: at(flare.onto, 0), edge: at(flare.onto, 0.5), crown: at(flare.onto - 1, flare.proud), over: at(-liner, flare.proud) };
    // (Planes for its ends: level at each foot, facing up into it; at the front corner, square to x, facing back into it.)
    const [p0, pn] = [path[0], path[path.length - 1]];
    const ends = {
      s0: off([a.r + 10, y, zb]), s1: off([a.r, y + 10, zb]),
      e0: k === "front" ? off([corner, y, frontArch.meets + 10]) : off([a.f + 10, y, zb]), e1: k === "front" ? off([corner, y + 10, frontArch.meets]) : off([a.f, y + 10, zb]),
    };
    const ring: Corner[] = [outline.in, outline.out, outline.edge, outline.crown, outline.over].map((n) => ({ node: n }));
    const f = part(`flare ${k} ${side}`, { ...outline, path, ends }, { at: placed({ x: [1, 0, 0], y: [0, s, 0] }), kind: "solid" });
    f.shapes = [sweep(ring, path as [never, never, ...never[]], petg)];
    return { part: f, start: plane(p0, ends.s0, ends.s1), end: k === "front" ? plane(pn, ends.e0, ends.e1) : plane(pn, ends.e0, ends.e1) };
  }));

  // ----- the seams ----------------------------------------------------------------------------------------------------

  // Edge to edge where plates meet at their edges (inner corner on inner corner), and an edge on a face where one stands
  // on another's inner face; fingered (or tabbed through slots) and TIG welded, butted where too short for fingers.
  return part("truck", (root) => {
    const fingered = { tabbed: true, fastening: welds.tig };
    const butted = { tabbed: false, fastening: welds.tig };
    const inner = { faces: ["under", "under"] as ["under", "under"] };
    const edge = (p: ReturnType<typeof shell>, a: string, b: string) => ({ plate: p, a, b });
    const meet = (name: string, x: ReturnType<typeof edge>, y: ReturnType<typeof edge>, how: object) => atCorner(root, name, x, y, { ...how, ...inner });
    const onto = (name: string, x: ReturnType<typeof edge>, p: ReturnType<typeof shell>, how: object) => onFace(root, name, x, p, { ...how, face: "under" });
    const seams: Record<string, object> = {};
    sides.forEach(([side], i) => {
      const [sd, wn, ch] = [sidePlates[i], windows[i], chamfers[i]];
      // (Along the beltline: its fingers in its middle, its ends plain, since toward the tail the window comes to a point
      // that a cut back under the side's first finger would cut off.)
      seams[`side ${side} to window`] = meet(`side ${side} to window`, edge(sd, "tail", "belt"), edge(wn, "tail", "belt"), { ...fingered, count: 11, length: mm(10) });
      seams[`side ${side} to hood`] = meet(`side ${side} to hood`, edge(sd, "belt", "front"), edge(hood, `belt ${side}`, `side ${side}`), corner - beltFrom >= 8 ? fingered : butted);
      seams[`side ${side} to chamfer`] = meet(`side ${side} to chamfer`, edge(sd, "front", "front arch meets"), edge(ch, "rear top", "meets"), fingered);
      seams[`hood to chamfer ${side}`] = meet(`hood to chamfer ${side}`, edge(hood, `side ${side}`, `bar ${side}`), edge(ch, "rear top", "front top"), Math.hypot(chamfer * Math.SQRT2, hoodZ(corner) - bar.z) >= 8 ? fingered : butted);
      seams[`band to chamfer ${side}`] = meet(`band to chamfer ${side}`, edge(band, `top ${side}`, `low ${side}`), edge(ch, "front top", "front low"), butted);
      seams[`chin to chamfer ${side}`] = meet(`chin to chamfer ${side}`, edge(chin, `top ${side}`, `bottom ${side}`), edge(ch, "front low", "low"), fingered);
      seams[`side ${side} to sill`] = meet(`side ${side} to sill`, edge(sd, "rear", "sill"), edge(sillPlate, `rear ${side}`, `sill ${side}`), fingered);
      seams[`window ${side} to hood`] = meet(`window ${side} to hood`, edge(wn, "belt", "peak"), edge(hood, `belt ${side}`, `peak ${side}`), fingered);
      seams[`window ${side} to roof`] = meet(`window ${side} to roof`, edge(wn, "peak", "cab"), edge(roof, `peak ${side}`, `back ${side}`), fingered);
      seams[`cab back in side ${side}`] = onto(`cab back in side ${side}`, edge(cabBack, `low ${side}`, `belt ${side}`), sd, fingered);
      seams[`cab back on window ${side}`] = onto(`cab back on window ${side}`, edge(cabBack, `belt ${side}`, `top ${side}`), wn, butted);
      seams[`floor on chamfer ${side}`] = onto(`floor on chamfer ${side}`, edge(floor, `side ${side}`, `chin ${side}`), ch, butted);
      // (The floor in the side, by tabs through slots in it: from the tail to the first tub's notch, between the notches,
      // and from the second to the front corner; butted where that is too short.)
      ([[`rear ${side}`, `rear tub ${side} a`], [`rear tub ${side} d`, `front tub ${side} a`], [`front tub ${side} d`, `side ${side}`]] as [string, string][]).forEach(([a, b], j) => {
        const x = [0, onFloor(arches.rear.ft, arches.rear.f), onFloor(arches.front.ft, arches.front.f)][j], to = [onFloor(arches.rear.rt, arches.rear.r), onFloor(arches.front.rt, arches.front.r), corner][j];
        seams[`floor in side ${side} ${j + 1}`] = onto(`floor in side ${side} ${j + 1}`, edge(floor, a, b), sd, to - x >= 8 ? fingered : butted);
      });
    });
    seams["hood to band"] = meet("hood to band", edge(hood, "bar L", "bar R"), edge(band, "top L", "top R"), fingered);
    seams["band to chin"] = meet("band to chin", edge(band, "low R", "low L"), edge(chin, "top R", "top L"), Math.atan2(chamfer, chinTop - zb) > (25 * Math.PI) / 180 ? fingered : butted);
    seams["hood to roof"] = meet("hood to roof", edge(hood, "peak L", "peak R"), edge(roof, "peak L", "peak R"), butted);
    seams["roof to cab back"] = meet("roof to cab back", edge(roof, "back L", "back R"), edge(cabBack, "top L", "top R"), fingered);
    seams["cab back on floor"] = onto("cab back on floor", edge(cabBack, "low R", "low L"), floor, fingered);
    seams["floor in chin"] = onto("floor in chin", edge(floor, "chin R", "chin L"), chin, chinTop - floorZ >= 2.5 ? fingered : butted);
    seams["floor in sill"] = onto("floor in sill", edge(floor, "rear L", "rear R"), sillPlate, fingered);
    for (const { k, side, s, long, top, front, rear, wall } of tubs) {
      const sd = sidePlates[s > 0 ? 0 : 1], n = `tub ${k} ${side}`, byLength = (l: number) => (l < 8 ? butted : fingered);
      seams[`${n} top on side`] = onto(`${n} top on side`, edge(top, "ft", "rt"), sd, butted);
      seams[`${n} front on side`] = onto(`${n} front on side`, edge(front, "ft", "ff"), sd, butted);
      seams[`${n} rear on side`] = onto(`${n} rear on side`, edge(rear, "rt", "rf"), sd, butted);
      seams[`${n} top to wall`] = meet(`${n} top to wall`, edge(top, "fti", "rti"), edge(wall, "ft", "rt"), fingered);
      seams[`${n} front to wall`] = meet(`${n} front to wall`, edge(front, "fti", "ffi"), edge(wall, "ft", "ff"), byLength(long.front));
      seams[`${n} rear to wall`] = meet(`${n} rear to wall`, edge(rear, "rti", "rfi"), edge(wall, "rt", "rf"), byLength(long.rear));
      seams[`${n} top to front`] = meet(`${n} top to front`, edge(top, "ft", "fti"), edge(front, "ft", "fti"), fingered);
      seams[`${n} top to rear`] = meet(`${n} top to rear`, edge(top, "rt", "rti"), edge(rear, "rt", "rti"), fingered);
      seams[`${n} wall on floor`] = onto(`${n} wall on floor`, edge(wall, "rf", "ff"), floor, fingered);
      seams[`${n} front on floor`] = onto(`${n} front on floor`, edge(front, "ff", "ffi"), floor, fingered);
      seams[`${n} rear on floor`] = onto(`${n} rear on floor`, edge(rear, "rf", "rfi"), floor, fingered);
    }
    // The glass: the side glass in front of the cab's back, in two by the door pillar; the windscreen up the hood from so
    // far along; the glass roof, all but its margin; the rear window in the cab's back over the beltline.
    const glaze = (name: string, p: ReturnType<typeof shell>, panes: (V[] | [number, number])[]) => cutOut(root, name, p, { panes: panes.map((q) => pane(p, q)), margin: mm(glass.margin), round: mm(glass.round) });
    sides.forEach(([side], i) => glaze(`window ${side} glass`, windows[i], [[cabX, glass.pillar.from], [glass.pillar.to, 300]]));
    glaze("windscreen", hood, [[0, glass.screen]]);
    glaze("glass roof", roof, [[0, 300]]);
    const beltZ = at.cabBelt(1)[2] + 2;
    glaze("rear window", cabBack, [[[cabX, -200, beltZ], [cabX, 200, beltZ], [cabX, 200, 200], [cabX, -200, 200]]]);
    // (The tailgate's ears on the gate, butted: they are small.)
    sides.forEach(([side], i) => { seams[`tailgate ear ${side} on gate`] = onto(`tailgate ear ${side} on gate`, edge(ears[i], "back low", "back top"), gate, butted); });
    const tailgate = part("tailgate hinge", { gate, ...Object.fromEntries(ears.map((e) => [e.name, e])) }, {
      at: placed({ origin: [mm(pivot[0]), mm(pivot[1]), mm(pivot[2])], joint: joints.hinge([0, -1, 0], free(deg(0), { min: deg(0), max: deg(90) })) }),
    });
    const body = Object.fromEntries([...sidePlates, ...windows, hood, roof, band, chin, ...chamfers, sillPlate, floor, cabBack, ...tubs.flatMap((b) => [b.top, b.front, b.rear, b.wall])].map((p) => [p.name, p]));
    // (The underbody wraps the front corners: what is outside each chamfer's inner face goes; then each wheel's well; then
    // each axle's bore, along its rod.)
    const underCuts = [
      ...chamfers.map((c) => cut(underbody, { tool: "planes", planes: [c.faces["-z"]] }, `corner ${c.name.slice(-1)}`)),
      ...axleParts.flatMap((a) => sides.map(([side]) => cut(underbody, { tool: "body", part: (a as unknown as Record<string, object>)[`well ${side}`] as never }, `well ${a.name.slice(5)} ${side}`))),
      ...archKeys.map((k) => cut(underbody, { tool: "bore", line: axleLines[k]!, r: mm(axleRod.bore / 2) }, `bore ${k}`)),
    ];
    // (Each rocker's and quarter's ends: on a tub's plate's face toward the wheel (its top, which looks into the body, a
    // thickness back), the quarter's back on the sill's inner face.)
    const tub = (k: string, s: number) => tubs.find((b) => b.k === k && b.s === s)!;
    const ends = sides.flatMap(([, s], i) => [
      endOn(rockers[i], tub("rear", s).front.faces["+z"], { gap: mm(-t) }),
      endOn(rockers[i], tub("front", s).rear.faces["+z"], { gap: mm(-t) }),
      endOn(quarters[i], sillPlate.faces["-z"]),
      endOn(quarters[i], tub("rear", s).rear.faces["+z"], { gap: mm(-t) }),
    ]);
    const flareEnds = flares.flatMap((f) => [endOn(f.part, f.start), endOn(f.part, f.end)]);
    // The drawing set, as the original's: the truck seen about and from below; a plate of each kind laid flat; the printed
    // parts; its seams and parts.
    const printed = [underbody, ...rockers, ...quarters, ...liners, ...lips, ...flares.map((f) => f.part), ...axleParts];
    const all = [...Object.values(body), tailgate, ...pins, ...printed];
    const sheets = [
      sheet("S-101", "Cybertruck",
        iso(all, { from: [1, 0.8, 0.5], title: "From the front and the left" }), iso(all, { from: [-1, -0.8, 0.6], title: "From behind and the right" }),
        back(all, { title: "From the left" }), side(all, { title: "From the front" }), plan(all, { title: "Plan" }),
        iso([floor, ...printed], { from: [0.6, 0.8, -0.6], title: "From below: the printed underbody, rockers and quarters, the axles and the wheels" })),
      sheet("S-102", "Plates, cut flat", ...[sidePlates[0], windows[0], hood, band, chin, chamfers[0], roof, cabBack, sillPlate, floor, gate, ears[0], tubs[0].wall, tubs[0].top, tubs[0].front].map((p) => laidFlat(p, { title: `${p.name}, laid flat` }))),
      sheet("S-103", "Printed",
        iso(printed, { from: [1, -0.8, 0.5], title: "The printed parts, from the front and the right" }), iso(printed, { from: [-0.6, -0.8, -0.6], title: "From below and behind" }),
        front(printed, { title: "From the right" })),
      sheet("S-401", "Schedules", schedule("Seams"), schedule("Parts")),
    ];
    return { ...body, ...Object.fromEntries([...rockers, ...quarters, ...liners, ...lips, ...flares.map((f) => f.part)].map((p) => [p.name, p])), lowerEnds: ends, flareEnds, tailgate, ...Object.fromEntries(pins.map((p) => [p.name, p])), underbody, ...Object.fromEntries(axleParts.map((a) => [a.name, a])), underCuts, seams, sheets };
  });
}
