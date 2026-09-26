/**
 * topo-cad scripting API.
 *
 * Models are plain TypeScript: dimensions are `const`s, variants are
 * functions, and every builder method returns a *new* value (nothing is
 * mutated), so templates can be shared and specialised freely.
 *
 * Lengths are metres. Use the unit helpers (`ft`, `inch`, `ftIn`, `mm`) at the
 * edges; arithmetic is ordinary number arithmetic.
 *
 * A script's default export is a `Building`; topo-cad expands it into the
 * topology/geometry model and renders drawings.
 */

// ----- units ---------------------------------------------------------------

export type Length = number;
/** Plan point or vector (x, y), metres. */
export type Point = readonly [number, number];

export const mm = (v: number): Length => v / 1000;
export const inch = (v: number): Length => v * 0.0254;
export const ft = (v: number): Length => v * 0.3048;
export const ftIn = (feet: number, inches: number): Length => ft(feet) + inch(inches);
/** Pounds per square foot → pascals. */
export const psf = (v: number): number => v * 47.880259;

// ----- materials -----------------------------------------------------------

export interface Grade {
  readonly species: string;
  readonly grade: string;
}
const grade = (species: string, g: string): Grade => ({ species, grade: g });
/** Douglas Fir-Larch, NDS Supplement Table 4A. */
export const DFL = {
  SelectStructural: grade("DFL", "Select Structural"),
  No1: grade("DFL", "No.1"),
  No2: grade("DFL", "No.2"),
  Stud: grade("DFL", "Stud"),
} as const;
/** Spruce-Pine-Fir. */
export const SPF = { No1No2: grade("SPF", "No.1/No.2"), Stud: grade("SPF", "Stud") } as const;

/** Nominal lumber size, e.g. `[2, 4]`. */
export type Nominal = readonly [number, number];

/** Dressed (actual) size of a nominal lumber dimension: `dressed(4)` = 3½". */
export function dressed(nominal: number): Length {
  const table: Record<number, number> = { 1: 0.75, 2: 1.5, 3: 2.5, 4: 3.5, 6: 5.5, 8: 7.25, 10: 9.25, 12: 11.25, 14: 13.25, 16: 15.25 };
  return inch(table[nominal] ?? nominal - 0.75);
}

// ----- openings ------------------------------------------------------------

interface OpeningSpec {
  readonly label: string;
  readonly kind: "door" | "window";
  readonly center?: Length;
  readonly width: Length;
  readonly height: Length;
  readonly head: Length;
  readonly header: readonly [number, number, number];
  readonly jacks: number;
  readonly header_flush: boolean;
}

/** A rough opening. Position it along its wall with `.at(center)`. */
export class Opening {
  private constructor(readonly spec: OpeningSpec) {}

  /** Door rough opening. `height` may be omitted with `.flushHeader()`. */
  static door(o: { label: string; width: Length; height?: Length }): Opening {
    const height = o.height ?? 0;
    return new Opening({ label: o.label, kind: "door", width: o.width, height, head: height, header: [2, 2, 8], jacks: 1, header_flush: false });
  }
  /** Window rough opening; `head` is the top of the RO above the wall base. */
  static window(o: { label: string; width: Length; height: Length; head: Length }): Opening {
    return new Opening({ label: o.label, kind: "window", width: o.width, height: o.height, head: o.head, header: [2, 2, 8], jacks: 1, header_flush: false });
  }
  private with(p: Partial<OpeningSpec>): Opening {
    return new Opening({ ...this.spec, ...p });
  }
  /** Centre of the opening, measured from the start of its wall. */
  at(center: Length): Opening {
    return this.with({ center });
  }
  /** Header as (plies, nominal thickness, nominal depth), e.g. `header(2, 2, 10)`. */
  header(plies: number, thick: number, depth: number): Opening {
    return this.with({ header: [plies, thick, depth] });
  }
  /** Header set tight under the top plate; RO height follows from wall height. */
  flushHeader(): Opening {
    return this.with({ header_flush: true });
  }
  /** Jack studs per side. `0` = header carried by king studs through nails only. */
  jacks(n: number): Opening {
    return this.with({ jacks: n });
  }
}

// ----- walls ---------------------------------------------------------------

interface WallSpec {
  readonly name: string;
  readonly height: Length;
  readonly studs: Nominal;
  readonly spacing: Length;
  readonly grade: Grade;
  readonly header_grade?: Grade;
  readonly justify: "exterior" | "center";
  readonly openings: readonly OpeningSpec[];
  readonly cap_breaks: readonly Length[];
}

/**
 * A stud wall *template*: framing properties without a position. Perimeters
 * (and other placers) give it a start and end.
 */
export class Wall {
  private constructor(readonly spec: WallSpec) {}

  static template(o: { height: Length; grade: Grade; studs?: Nominal; spacing?: Length }): Wall {
    return new Wall({
      name: "",
      height: o.height,
      studs: o.studs ?? [2, 4],
      spacing: o.spacing ?? inch(16),
      grade: o.grade,
      justify: "exterior",
      openings: [],
      cap_breaks: [],
    });
  }
  private with(p: Partial<WallSpec>): Wall {
    return new Wall({ ...this.spec, ...p });
  }
  named(name: string): Wall {
    return this.with({ name });
  }
  height(h: Length): Wall {
    return this.with({ height: h });
  }
  studs(size: Nominal, spacing?: Length): Wall {
    return this.with({ studs: size, spacing: spacing ?? this.spec.spacing });
  }
  grade(g: Grade): Wall {
    return this.with({ grade: g });
  }
  headerGrade(g: Grade): Wall {
    return this.with({ header_grade: g });
  }
  /** Wall line on the exterior face of framing (default) or its centre. */
  justify(j: "exterior" | "center"): Wall {
    return this.with({ justify: j });
  }
  opening(o: Opening): Wall {
    if (o.spec.center === undefined) throw new Error(`opening ${o.spec.label} has no position: use .at(center)`);
    return this.with({ openings: [...this.spec.openings, o.spec] });
  }
  /** Interrupt the cap plate at `x` (where a partition tees in and laps over). */
  capBreak(x: Length): Wall {
    return this.with({ cap_breaks: [...this.spec.cap_breaks, x] });
  }
}

// ----- perimeters ----------------------------------------------------------

/** What a perimeter segment is: a wall (from a template) or open (no wall). */
export type Side = Wall | "open";

interface Segment {
  readonly to: Point;
  readonly side: WallSpec | null;
}

/**
 * A footprint drawn as a plan path. Each segment is a wall or open side.
 * Direction (clockwise or not) decides the interior; corners, plate laps and
 * layout insets are derived.
 */
export class Perimeter {
  private constructor(
    readonly name: string,
    readonly start: Point,
    readonly z: Length,
    readonly segments: readonly Segment[],
    readonly closed: boolean,
  ) {}

  static start(name: string, at: Point, opts: { z?: Length } = {}): Perimeter {
    return new Perimeter(name, at, opts.z ?? 0, [], false);
  }
  /** Current end point of the path. */
  get end(): Point {
    return this.segments.length ? this.segments[this.segments.length - 1].to : this.start;
  }
  private push(to: Point, side: Side): Perimeter {
    if (this.closed) throw new Error(`perimeter ${this.name} is already closed`);
    const spec = side === "open" ? null : side.spec;
    return new Perimeter(this.name, this.start, this.z, [...this.segments, { to, side: spec }], false);
  }
  /** Straight segment to an absolute point. */
  to(p: Point, side: Side): Perimeter {
    return this.push(p, side);
  }
  /** Straight segment by a relative offset. */
  by(d: Point, side: Side): Perimeter {
    const [x, y] = this.end;
    return this.push([x + d[0], y + d[1]], side);
  }
  /** Straight segment of `length` at `degrees` counter-clockwise from +x. */
  go(length: Length, degrees: number, side: Side): Perimeter {
    const a = (degrees * Math.PI) / 180;
    return this.by([length * Math.cos(a), length * Math.sin(a)], side);
  }
  /** Final segment back to the start. */
  close(side: Side): Perimeter {
    const p = this.push(this.start, side);
    return new Perimeter(p.name, p.start, p.z, p.segments, true);
  }
  /** A member of one of this perimeter's walls, e.g. `wallMember("Wall A", "cap_plate")`. */
  wallMember(wall: string, name: string): MemberRef {
    return new MemberRef(`${this.name}/${wall}/${name}`);
  }
  /** The same footprint translated (plan offset, plus optional height change). */
  moved(d: Point | Point3): Perimeter {
    const mv = (p: Point): Point => [p[0] + d[0], p[1] + d[1]];
    const dz = (d as readonly number[])[2] ?? 0;
    return new Perimeter(this.name, mv(this.start), this.z + dz, this.segments.map((g) => ({ ...g, to: mv(g.to) })), this.closed);
  }
  named(name: string): Perimeter {
    return new Perimeter(name, this.start, this.z, this.segments, this.closed);
  }
  toJSON() {
    return { type: "perimeter", name: this.name, start: this.start, z: this.z, segments: this.segments, closed: this.closed };
  }
}

// ----- geometry utilities (truss plane: [s, z]; plan: [x, y]) ---------------

/** Point a fraction `t` of the way from `a` to `b`. */
export const lerp = (a: Point, b: Point, t: number): Point => [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
/** Point on the line through `a` and `b` at first coordinate `x` (e.g. a chord at a given s). */
export const atX = (a: Point, b: Point, x: number): Point => lerp(a, b, (x - a[0]) / (b[0] - a[0]));
/** Point on the line through `a` and `b` at distance `d` from `a`. */
export const along = (a: Point, b: Point, d: number): Point => lerp(a, b, d / Math.hypot(b[0] - a[0], b[1] - a[1]));
/** Intersection of lines a1–a2 and b1–b2 (throws if parallel). */
export function intersect(a1: Point, a2: Point, b1: Point, b2: Point): Point {
  const [dx1, dy1, dx2, dy2] = [a2[0] - a1[0], a2[1] - a1[1], b2[0] - b1[0], b2[1] - b1[1]];
  const den = dx1 * dy2 - dy1 * dx2;
  if (Math.abs(den) < 1e-12) throw new Error("intersect: lines are parallel");
  const t = ((b1[0] - a1[0]) * dy2 - (b1[1] - a1[1]) * dx2) / den;
  return [a1[0] + dx1 * t, a1[1] + dy1 * t];
}
/** Pitch as rise per unit run: `pitch(6, 12)` = 6:12. */
export const pitch = (rise: number, run: number = 12): number => rise / run;

// ----- truss shapes ----------------------------------------------------------

export type TrussRole = "top_chord" | "bottom_chord" | "web";
interface ShapeMemberSpec {
  readonly role: TrussRole;
  readonly path: readonly string[];
  readonly priority?: number;
  readonly anchor?: number;
  readonly size?: Nominal;
}
interface ShapeDimSpec {
  readonly a: Point;
  readonly b: Point;
  readonly side: Point;
  readonly tier: number;
}
interface ShapeSpec {
  readonly name: string;
  readonly span: Length;
  readonly pitch?: number;
  readonly points: readonly { readonly name: string; readonly s: number; readonly z: number }[];
  readonly members: readonly ShapeMemberSpec[];
  readonly dims?: readonly ShapeDimSpec[];
}

/** Which side of the measured points a dimension line is drawn on. */
export type DimSide = "below" | "above" | "left" | "right";
const sideVec: Record<DimSide, Point> = { below: [0, -1], above: [0, 1], left: [-1, 0], right: [1, 0] };

declare const __topo_truss_standard: (request: string) => string;

/**
 * A truss as data: named points in the truss plane (`[s, z]`, s along the span
 * from the left heel at [0, 0]) and members as straight paths through them.
 * Standard types come from the built-in generators; every shape can be edited
 * (move points, add or remove webs, resize members) or written from scratch.
 *
 * Standard point names: heels `H0`/`H1`, peak `P`, overhang tails `X0`/`X1`,
 * top-chord panel points `T1`, `T2`… (mirrored `T1'`…), bottom `B1`, `B2`…
 */
export class TrussShape {
  private constructor(readonly spec: ShapeSpec) {}

  private static standard(req: object): TrussShape {
    const r = JSON.parse(__topo_truss_standard(JSON.stringify(req)));
    if (r.error) throw new Error(r.error);
    return new TrussShape(r);
  }
  static fink(o: { span: Length; pitch: number; overhang?: Length }): TrussShape {
    return TrussShape.standard({ kind: "fink", ...o, overhang: o.overhang ?? 0 });
  }
  static fan(o: { span: Length; pitch: number; overhang?: Length }): TrussShape {
    return TrussShape.standard({ kind: "fan", ...o, overhang: o.overhang ?? 0 });
  }
  static kingPost(o: { span: Length; pitch: number; overhang?: Length }): TrussShape {
    return TrussShape.standard({ kind: "king_post", ...o, overhang: o.overhang ?? 0 });
  }
  /** Verticals plus diagonals rising toward the centre. `panels` even, ≥ 4. */
  static howe(o: { span: Length; pitch: number; panels: number; overhang?: Length }): TrussShape {
    return TrussShape.standard({ kind: "howe", ...o, overhang: o.overhang ?? 0 });
  }
  /** Verticals plus diagonals falling toward the centre. `panels` even, ≥ 4. */
  static pratt(o: { span: Length; pitch: number; panels: number; overhang?: Length }): TrussShape {
    return TrussShape.standard({ kind: "pratt", ...o, overhang: o.overhang ?? 0 });
  }
  /** Sloped bottom chords (vaulted ceiling); `bottomPitch` < `pitch`. */
  static scissors(o: { span: Length; pitch: number; bottomPitch: number; overhang?: Length }): TrussShape {
    return TrussShape.standard({ kind: "scissors", span: o.span, pitch: o.pitch, bottom_pitch: o.bottomPitch, overhang: o.overhang ?? 0 });
  }
  /** Single slope rising from H0 to the high end E, with an end post. */
  static mono(o: { span: Length; pitch: number; panels: number; overhang?: Length }): TrussShape {
    return TrussShape.standard({ kind: "mono", ...o, overhang: o.overhang ?? 0 });
  }
  /** A shape written from scratch. */
  static custom(o: {
    name: string;
    span: Length;
    /** Nominal pitch, for drawing labels only. */
    pitch?: number;
    points: Record<string, Point>;
    topChords: readonly (readonly string[])[];
    bottomChords: readonly (readonly string[])[];
    webs: readonly (readonly [string, string])[];
  }): TrussShape {
    const points = Object.entries(o.points).map(([name, [s, z]]) => ({ name, s, z }));
    const members: ShapeMemberSpec[] = [
      ...o.topChords.map((path) => ({ role: "top_chord" as const, path })),
      ...o.bottomChords.map((path) => ({ role: "bottom_chord" as const, path })),
      ...o.webs.map((path) => ({ role: "web" as const, path })),
    ];
    return new TrussShape({ name: o.name, span: o.span, pitch: o.pitch, points, members });
  }
  /**
   * Symmetric pitched shape from its left half: top-chord panel points by `s`,
   * bottom panel points by `[s, z]`, and webs; the right half is mirrored
   * (names get a `'`). Points at the centre line are not mirrored.
   */
  static symmetric(o: {
    name: string;
    span: Length;
    pitch: number;
    overhang?: Length;
    top?: Record<string, number>;
    bottom?: Record<string, number>;
    webs: readonly (readonly [string, string])[];
  }): TrussShape {
    const { span: l, pitch: k } = o;
    const ov = o.overhang ?? 0;
    const pts: Record<string, Point> = { H0: [0, 0], H1: [l, 0], P: [l / 2, (l / 2) * k] };
    if (ov > 0) Object.assign(pts, { X0: [-ov, -ov * k], X1: [l + ov, -ov * k] });
    const mid = (s: number) => Math.abs(s - l / 2) < 1e-9;
    const mir = (n: string) => (n === "P" ? "P" : n === "H0" ? "H1" : n === "H1" ? "H0" : n === "X0" ? "X1" : n === "X1" ? "X0" : mid(pts[n]?.[0] ?? -1) ? n : `${n}'`);
    const top = Object.entries(o.top ?? {}).sort((a, b) => a[1] - b[1]);
    for (const [n, s] of top) {
      pts[n] = [s, s * k];
      pts[`${n}'`] = [l - s, s * k];
    }
    const bottom = Object.entries(o.bottom ?? {}).sort((a, b) => a[1] - b[1]);
    for (const [n, s] of bottom) {
      pts[n] = [s, 0];
      if (!mid(s)) pts[`${n}'`] = [l - s, 0];
    }
    const tails = ov > 0 ? [["X0"], ["X1"]] : [[], []];
    const left = [...tails[0], "H0", ...top.map(([n]) => n), "P"];
    const right = [...tails[1], "H1", ...top.map(([n]) => `${n}'`), "P"];
    const bot = [
      "H0",
      ...bottom.map(([n]) => n),
      ...bottom.filter(([, s]) => !mid(s)).reverse().map(([n]) => `${n}'`),
      "H1",
    ];
    const webs: [string, string][] = [];
    const key = (a: string, b: string) => [a, b].sort().join("|");
    const seen = new Set<string>();
    for (const [a, b] of o.webs) {
      for (const w of [[a, b], [mir(a), mir(b)]] as [string, string][]) {
        if (!seen.has(key(...w))) {
          seen.add(key(...w));
          webs.push(w);
        }
      }
    }
    const shape = TrussShape.custom({ name: o.name, span: l, points: pts, topChords: [left, right], bottomChords: [bot], webs });
    return new TrussShape({ ...shape.spec, pitch: k });
  }

  private with(p: Partial<ShapeSpec>): TrussShape {
    return new TrussShape({ ...this.spec, ...p });
  }
  /** Coordinates of a named point. */
  point(name: string): Point {
    const p = this.spec.points.find((q) => q.name === name);
    if (!p) throw new Error(`truss ${this.spec.name}: no point ${name}`);
    return [p.s, p.z];
  }
  get pointNames(): string[] {
    return this.spec.points.map((p) => p.name);
  }
  named(name: string): TrussShape {
    return this.with({ name });
  }
  /** Add a point, or move an existing one (chords through it must stay straight). */
  withPoint(name: string, at: Point): TrussShape {
    const rest = this.spec.points.filter((p) => p.name !== name);
    return this.with({ points: [...rest, { name, s: at[0], z: at[1] }] });
  }
  /** Add a point on a chord: inserts it into the chord path containing `a` then `b`. */
  withChordPoint(name: string, a: string, b: string, t: number): TrussShape {
    const at = lerp(this.point(a), this.point(b), t);
    const members = this.spec.members.map((m) => {
      const i = m.path.indexOf(a);
      const j = m.path.indexOf(b);
      if (m.role === "web" || i < 0 || j < 0 || Math.abs(i - j) !== 1) return m;
      const path = [...m.path];
      path.splice(Math.max(i, j), 0, name);
      return { ...m, path };
    });
    return this.withPoint(name, at).with({ members });
  }
  /**
   * Add a drawing dimension between two points (names or `[s, z]`
   * coordinates). Once any are added they replace the automatic ones, so a
   * truss can be dimensioned the way it was measured.
   */
  dim(a: string | Point, b: string | Point, side: DimSide, tier = 0): TrussShape {
    const at = (p: string | Point): Point => (typeof p === "string" ? this.point(p) : p);
    const d: ShapeDimSpec = { a: at(a), b: at(b), side: sideVec[side], tier };
    return this.with({ dims: [...(this.spec.dims ?? []), d] });
  }
  web(a: string, b: string, o: { size?: Nominal } = {}): TrussShape {
    return this.with({ members: [...this.spec.members, { role: "web", path: [a, b], ...o }] });
  }
  withoutWeb(a: string, b: string): TrussShape {
    const hit = (m: ShapeMemberSpec) => m.role === "web" && m.path.length === 2 && m.path.includes(a) && m.path.includes(b);
    if (!this.spec.members.some(hit)) throw new Error(`truss ${this.spec.name}: no web ${a}–${b}`);
    return this.with({ members: this.spec.members.filter((m) => !hit(m)) });
  }
  /** All webs removed (to redraw a pattern on standard chords). */
  withoutWebs(): TrussShape {
    return this.with({ members: this.spec.members.filter((m) => m.role !== "web") });
  }
  /** Resize every member of a role, e.g. `.sized("bottom_chord", [2, 6])`. */
  sized(role: TrussRole, size: Nominal): TrussShape {
    return this.with({ members: this.spec.members.map((m) => (m.role === role ? { ...m, size } : m)) });
  }
  toJSON() {
    return this.spec;
  }
}

// ----- roofs ---------------------------------------------------------------

type LoadKind = "dead" | "live" | "roof_live" | "snow";
interface AreaLoad {
  readonly kind: LoadKind;
  readonly pressure: number;
}

/** A run of identical trusses at a spacing, with flush gable-end trusses. */
export class TrussRoof {
  private constructor(readonly spec: {
    readonly name: string;
    readonly origin: Point;
    readonly span_dir: Point;
    readonly shape: TrussShape;
    readonly length: Length;
    readonly spacing: Length;
    readonly chord: Nominal;
    readonly web: Nominal;
    readonly grade: Grade;
    readonly height?: Length;
    readonly bears_on: readonly string[];
    readonly loads: readonly AreaLoad[];
  }) {}

  /**
   * `origin`: outside corner of a bearing wall where the first truss's left
   * heel sits; `spanDir`: direction the trusses span; `length`: along the ridge.
   */
  static of(
    shape: TrussShape,
    o: { name?: string; origin: Point; spanDir: Point; length: Length; spacing?: Length; grade: Grade; chord?: Nominal; web?: Nominal; height?: Length },
  ): TrussRoof {
    return new TrussRoof({
      name: o.name ?? "Roof trusses",
      origin: o.origin,
      span_dir: o.spanDir,
      shape,
      length: o.length,
      spacing: o.spacing ?? inch(24),
      chord: o.chord ?? [2, 4],
      web: o.web ?? [2, 4],
      grade: o.grade,
      height: o.height,
      bears_on: [],
      loads: [],
    });
  }
  /** Fink trusses (shorthand for `TrussRoof.of(TrussShape.fink(...), ...)`). */
  static fink(o: {
    name?: string;
    origin: Point;
    spanDir: Point;
    span: Length;
    length: Length;
    pitch: number;
    spacing?: Length;
    overhang?: Length;
    grade: Grade;
    height?: Length;
  }): TrussRoof {
    return TrussRoof.of(TrussShape.fink({ span: o.span, pitch: o.pitch, overhang: o.overhang ?? inch(12) }), o);
  }
  /** Trusses bear on the cap plates of these perimeters (sets height if not given). */
  bearingOn(...ps: Perimeter[]): TrussRoof {
    return new TrussRoof({ ...this.spec, bears_on: [...this.spec.bears_on, ...ps.map((p) => p.name)] });
  }
  /** Uniform load on the roof, in pascals (use `psf`). */
  load(kind: LoadKind, pressure: number): TrussRoof {
    return new TrussRoof({ ...this.spec, loads: [...this.spec.loads, { kind, pressure }] });
  }
  get name(): string {
    return this.spec.name;
  }
  /** A member of the `i`-th truss (1-based; 1 and the last are the gable ends), by its shape name. */
  trussMember(i: number, name: string): MemberRef {
    return new MemberRef(`${this.spec.name}/T${i}/${name}`);
  }
  /** Plan offset (a z component is ignored: height follows the bearing walls). */
  moved(d: Point | Point3): TrussRoof {
    return new TrussRoof({ ...this.spec, origin: [this.spec.origin[0] + d[0], this.spec.origin[1] + d[1]] });
  }
  named(name: string): TrussRoof {
    return new TrussRoof({ ...this.spec, name });
  }
  toJSON() {
    return { type: "truss_roof", ...this.spec };
  }
}

// ----- placement and custom assemblies --------------------------------------

/** 3D point or vector (x, y, z), metres. */
export type Point3 = readonly [number, number, number];

/** A rigid placement: local frame axes and origin in world coordinates. */
export class Placement {
  private constructor(readonly origin: Point3, readonly x: Point3, readonly y: Point3, readonly z: Point3) {}
  static readonly identity = new Placement([0, 0, 0], [1, 0, 0], [0, 1, 0], [0, 0, 1]);
  /** Placement at `origin` with the local x axis turned `degrees` about +z. */
  static at(origin: Point3, degrees = 0): Placement {
    return Placement.identity.rotated(degrees).moved(origin);
  }
  moved(d: Point3): Placement {
    const o = this.origin;
    return new Placement([o[0] + d[0], o[1] + d[1], o[2] + d[2]], this.x, this.y, this.z);
  }
  /** Rotation by `degrees` about the vertical axis through `about` (default: world origin). */
  rotated(degrees: number, about: Point3 = [0, 0, 0]): Placement {
    const a = (degrees * Math.PI) / 180;
    const [c, s] = [Math.cos(a), Math.sin(a)];
    const r = (v: Point3): Point3 => [c * v[0] - s * v[1], s * v[0] + c * v[1], v[2]];
    const o = this.origin;
    const rel: Point3 = [o[0] - about[0], o[1] - about[1], o[2] - about[2]];
    const ro = r(rel);
    return new Placement([ro[0] + about[0], ro[1] + about[1], ro[2] + about[2]], r(this.x), r(this.y), r(this.z));
  }
  toJSON() {
    return { origin: this.origin, x: this.x, y: this.y, z: this.z };
  }
}

export type Joints = "nailed" | "truss_plate" | "none";

interface AssemblyMemberSpec {
  readonly role: string;
  readonly path: readonly string[];
  readonly size: Nominal;
  readonly plies: number;
  readonly grade: Grade;
  readonly depth?: Point3;
  readonly anchor?: readonly [number, number];
  readonly priority: number;
}

/**
 * Any custom piece of topology: named points and members in a local frame.
 * Members meeting at a shared point are connected (and resolved into butt,
 * lap, mitre… joints like everything else). Place it with `placed`/`moved`/
 * `rotated`, and repeat it with `repeat`.
 */
export class Assembly {
  private constructor(
    readonly name: string,
    readonly placement: Placement,
    readonly points: readonly { readonly name: string; readonly at: Point3 }[],
    readonly members: readonly AssemblyMemberSpec[],
    readonly joints_: Joints,
    readonly bearsOn: readonly string[],
    readonly supports: readonly string[],
  ) {}

  static named(name: string): Assembly {
    return new Assembly(name, Placement.identity, [], [], "nailed", [], []);
  }
  private copy(
    p: Partial<{ name: string; placement: Placement; points: Assembly["points"]; members: Assembly["members"]; joints: Joints; bearsOn: readonly string[]; supports: readonly string[] }>,
  ): Assembly {
    return new Assembly(
      p.name ?? this.name,
      p.placement ?? this.placement,
      p.points ?? this.points,
      p.members ?? this.members,
      p.joints ?? this.joints_,
      p.bearsOn ?? this.bearsOn,
      p.supports ?? this.supports,
    );
  }
  /** These points bear on the foundation (e.g. post bases on footings). */
  supportedAt(...points: string[]): Assembly {
    for (const n of points) if (!this.points.some((p) => p.name === n)) throw new Error(`assembly ${this.name}: unknown point ${n}`);
    return this.copy({ supports: [...this.supports, ...points] });
  }
  named(name: string): Assembly {
    return this.copy({ name });
  }
  /** Add a named point (local coordinates). */
  point(name: string, at: Point3): Assembly {
    if (this.points.some((p) => p.name === name)) throw new Error(`assembly ${this.name}: duplicate point ${name}`);
    return this.copy({ points: [...this.points, { name, at }] });
  }
  /** Add a member through named points (straight, in order). */
  member(
    role: string,
    path: readonly string[],
    o: { size: Nominal; grade: Grade; plies?: number; depth?: Point3; anchor?: readonly [number, number]; priority?: number },
  ): Assembly {
    for (const n of path) if (!this.points.some((p) => p.name === n)) throw new Error(`assembly ${this.name}: unknown point ${n}`);
    const m: AssemblyMemberSpec = { role, path, size: o.size, plies: o.plies ?? 1, grade: o.grade, depth: o.depth, anchor: o.anchor, priority: o.priority ?? 0 };
    return this.copy({ members: [...this.members, m] });
  }
  /** How members that meet are fastened (default `nailed`). */
  joints(kind: Joints): Assembly {
    return this.copy({ joints: kind });
  }
  /** Connect to the cap plates of these perimeters where axes meet. */
  bearingOn(...ps: Perimeter[]): Assembly {
    return this.copy({ bearsOn: [...this.bearsOn, ...ps.map((p) => p.name)] });
  }
  placed(p: Placement): Assembly {
    return this.copy({ placement: p });
  }
  moved(d: Point3): Assembly {
    return this.copy({ placement: this.placement.moved(d) });
  }
  rotated(degrees: number, about?: Point3): Assembly {
    return this.copy({ placement: this.placement.rotated(degrees, about) });
  }
  toJSON() {
    return {
      type: "assembly",
      name: this.name,
      placement: this.placement,
      points: this.points,
      members: this.members,
      joints: this.joints_,
      bears_on: this.bearsOn,
      supports: this.supports,
    };
  }
}

/** Anything that can be moved and renamed. */
export interface Placeable<T> {
  moved(d: Point3): T;
  named(name: string): T;
  readonly name: string;
}

/**
 * `count` copies of `item`, the i-th moved by `i × step` and named
 * `"<name> #<i+1>"`. Copies that land on shared points connect automatically.
 */
export function repeat<T extends Placeable<T>>(item: T, count: number, step: Point3): T[] {
  return Array.from({ length: count }, (_, i) => item.moved([step[0] * i, step[1] * i, step[2] * i]).named(`${item.name} #${i + 1}`));
}

// ----- unknowns and field measurements --------------------------------------

interface UnknownSpec {
  readonly name: string;
  readonly guess: number;
  readonly unit: "length" | "ratio";
  readonly min?: number;
  readonly max?: number;
}
const unknownRegistry: UnknownSpec[] = [];

/**
 * A parameter to be fitted to field measurements. Returns the solver's
 * current value while solving, otherwise `guess`. Use it like any number.
 */
export function unknown(name: string, guess: number, o: { unit?: "length" | "ratio"; min?: number; max?: number } = {}): number {
  if (unknownRegistry.some((u) => u.name === name)) throw new Error(`unknown "${name}" declared twice`);
  unknownRegistry.push({ name, guess, unit: o.unit ?? "ratio", min: o.min, max: o.max });
  const params = (globalThis as { __topo_params?: Record<string, number> }).__topo_params;
  return params && name in params ? params[name] : guess;
}

/** One plane of a member solid, named through the member's stable path. */
export type PlaneRef =
  | { readonly kind: "face"; readonly member: string; readonly side: FaceSide }
  | { readonly kind: "mid"; readonly member: string; readonly axis: "depth" | "width" };
/** `top`/`bottom` (depth direction), `front`/`back` (width), `start`/`end` (cut ends along the path). */
export type FaceSide = "top" | "bottom" | "front" | "back" | "start" | "end";
/** Intersection of 1–3 planes: a face, an edge/corner line, or a point. */
export interface Feature {
  readonly planes: readonly PlaneRef[];
}

/** A member, by the `/`-separated tail of its path (e.g. `"T2/bottom_chord.F-H1"`). */
export class MemberRef {
  constructor(readonly path: string) {}
  face(side: FaceSide): PlaneRef {
    return { kind: "face", member: this.path, side };
  }
  /** Mid-plane across the member's depth or width (e.g. a wall centreline). */
  mid(axis: "depth" | "width"): PlaneRef {
    return { kind: "mid", member: this.path, axis };
  }
}
export const member = (path: string): MemberRef => new MemberRef(path);
/** Where faces meet: two faces make an edge/corner line, three a point. */
export const meet = (...planes: PlaneRef[]): Feature => ({ planes });
const asFeature = (f: Feature | PlaneRef): Feature => ("planes" in f ? f : { planes: [f] });

export type Quantity =
  | { readonly kind: "horizontal"; readonly a: Feature; readonly b: Feature }
  | { readonly kind: "vertical"; readonly a: Feature; readonly b: Feature }
  | { readonly kind: "length"; readonly member: string; readonly how: "long" | "centreline" };
export const horizontal = (a: Feature | PlaneRef, b: Feature | PlaneRef): Quantity => ({ kind: "horizontal", a: asFeature(a), b: asFeature(b) });
export const vertical = (a: Feature | PlaneRef, b: Feature | PlaneRef): Quantity => ({ kind: "vertical", a: asFeature(a), b: asFeature(b) });
/** `long`: lumber length (long point to long point); `centreline`: between the end cuts along its centreline. */
export const lengthOf = (m: MemberRef, how: "long" | "centreline"): Quantity => ({ kind: "length", member: m.path, how });

export interface Measurement {
  readonly name: string;
  readonly quantity: Quantity;
  readonly value: Length;
  readonly tolerance?: Length;
  readonly note?: string;
}
/** A tape reading of `quantity`. `tolerance` (default 1/16") weights it in the fit. */
export const measured = (name: string, quantity: Quantity, value: Length, o: { tolerance?: Length; note?: string } = {}): Measurement => ({
  name,
  quantity,
  value,
  ...o,
});

// ----- building ------------------------------------------------------------

export interface Info {
  readonly number: string;
  readonly client: string;
  readonly address: string;
  readonly designer: string;
  readonly date: string;
  readonly design_basis: readonly string[];
  readonly notes: readonly string[];
}

export type Item = Perimeter | TrussRoof | Assembly;

/** The whole model: what a script exports as its default. */
export class Building {
  private constructor(
    readonly name: string,
    readonly info_: Partial<Info>,
    readonly items: readonly Item[],
    readonly measurements: readonly Measurement[],
  ) {}

  static named(name: string): Building {
    return new Building(name, {}, [], []);
  }
  info(i: Partial<Info>): Building {
    return new Building(this.name, { ...this.info_, ...i }, this.items, this.measurements);
  }
  /** Add items (arrays, e.g. from `repeat`, are flattened). */
  add(...items: (Item | readonly Item[])[]): Building {
    return new Building(this.name, this.info_, [...this.items, ...items.flat()], this.measurements);
  }
  /** Field measurements; `unknown`s are fitted to them when the model is run. */
  measure(...ms: (Measurement | readonly Measurement[])[]): Building {
    return new Building(this.name, this.info_, this.items, [...this.measurements, ...ms.flat()]);
  }
  toJSON() {
    return { name: this.name, info: this.info_, items: this.items, measurements: this.measurements, unknowns: unknownRegistry };
  }
}
