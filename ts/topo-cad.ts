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
/** Pounds per linear foot → newtons per metre. */
export const plf = (v: number): number => v * 14.593903;
/** Pounds (force) → newtons. */
export const lbf = (v: number): number => v * 4.4482216;
/** Kips → newtons. */
export const kip = (v: number): number => v * 4448.2216;

// ----- reference data (tables with provenance, see crates/topo-data) ----------

declare const __topo_data: (table: string) => string;
interface MaterialWeightRow {
  readonly key: string;
  readonly description: string;
  readonly psf: number;
  readonly verified: boolean;
  readonly thickness_in?: number;
  readonly ribs?: { readonly width_in: number; readonly spacing_in: number; readonly along: "slope" | "run" };
}
interface DataTable<T> {
  readonly source: { readonly publication: string; readonly edition: string; readonly table: string };
  readonly rows: readonly T[];
}
/** A reference table by name (e.g. `"material-weights"`, `"nds-lumber"`). */
export function dataTable<T = unknown>(name: string): DataTable<T> {
  const t = JSON.parse(__topo_data(name));
  if (t === null) throw new Error(`no data table "${name}"`);
  return t;
}

// ----- site hazards (authored per jurisdiction; see examples/site-template.ts) ----

/** A value with where it came from (a URL, table, or document). */
export interface Sourced<T> {
  readonly value: T;
  readonly source: string;
}

/**
 * Design criteria for a site. Most jurisdictions publish these as IRC Table
 * R301.2(1) ("climatic and geographic design criteria"); seismic values can
 * also come from the USGS design-maps service. Every value names its source.
 */
export interface SiteHazards {
  readonly jurisdiction: string;
  /** Edition the values belong to: ASCE 7-22 ground snow is strength-level (ASD uses 0.7S). */
  readonly asce7: "7-16" | "7-22";
  readonly riskCategory: 1 | 2 | 3 | 4;
  /** Ground snow load p_g (psf). */
  readonly groundSnow?: Sourced<number>;
  /** Ultimate design wind speed (mph) and exposure category. */
  readonly windSpeed?: Sourced<number>;
  readonly exposure?: Sourced<"B" | "C" | "D">;
  /** Seismic design category, and S_DS / S_D1 where looked up (USGS design maps). */
  readonly seismic?: Sourced<{ readonly sds?: number; readonly sd1?: number; readonly sdc: string }>;
  readonly frostDepth?: Sourced<Length>;
  readonly notes?: readonly string[];
}

/**
 * Flat-roof snow load for a roof from site hazards, on plan area:
 * ASCE 7-16: p_f = 0.7 C_e C_t I_s p_g;  ASCE 7-22: p_f = 0.7 C_e C_t p_g.
 * `ce` (exposure, ASCE 7 Table 7.3-1) and `ct` (thermal, Table 7.3-2 / 7.3-3)
 * are yours to choose (an unheated garage is typically C_t = 1.2). Slope
 * factor C_s = 1 is assumed (conservative for ordinary slopes); minimum snow
 * loads, drifts and unbalanced loads are not applied.
 */
export function roofSnow(site: SiteHazards, o: { ce: number; ct: number; is?: number }): DeadLoad {
  if (!site.groundSnow) throw new Error(`${site.jurisdiction}: no ground snow load given`);
  const pg = site.groundSnow.value;
  const is = site.asce7 === "7-16" ? (o.is ?? [0.8, 1.0, 1.1, 1.2][site.riskCategory - 1]) : 1;
  const pf = 0.7 * o.ce * o.ct * is * pg;
  const eq = site.asce7 === "7-16" ? `0.7 · ${o.ce} · ${o.ct} · ${is} · ${pg}` : `0.7 · ${o.ce} · ${o.ct} · ${pg}`;
  return {
    pressure: psf(pf),
    label: `Snow p_f = ${eq} = ${pf.toFixed(1)} psf (ASCE ${site.asce7} Eq. 7.3-1; p_g from ${site.groundSnow.source}; C_s = 1 assumed; drift/unbalanced not included)`,
  };
}

/** One layer of a build-up (top, weather side, first in a list). */
export interface Layer {
  readonly name: string;
  /** Weight, psf of surface. */
  readonly psf: number;
  /** Thickness (m), for drawing. */
  readonly thickness: number;
  /** Parallel strips (spaced boards, standing-seam ribs): width and spacing (m), and their direction; `sheet` if a continuous sheet lies under them. */
  readonly strips?: { readonly width: number; readonly spacing: number; readonly along: "slope" | "run"; readonly sheet: boolean };
  readonly verified: boolean;
}

/**
 * A layer of boards, as a `layers(...)` item: weight from the species'
 * specific gravity (NDS Table 12.3.3A, via the lumber table) at about 12 %
 * moisture, the dressed size, and the board spacing (default: laid tight).
 * Boards run along the ridge (across the rafters or top chords).
 */
export function boards(size: Nominal, o: { grade?: Grade; spacing?: Length } = {}): Layer {
  const grade = o.grade ?? DFL.No2;
  const t = dataTable<{ species: string; grade: string; g: number }>("nds-lumber");
  const row = t.rows.find((r) => r.species === grade.species && r.grade === grade.grade) ?? t.rows.find((r) => r.species === grade.species);
  if (!row) throw new Error(`no specific gravity for ${grade.species}`);
  const [tk, wd] = [dressed(size[0]) / 0.0254, dressed(size[1]) / 0.0254]; // inches
  const pcf = 62.4 * row.g * 1.12;
  const spacingIn = o.spacing === undefined ? wd : o.spacing / 0.0254;
  const psfValue = (pcf * (tk / 12) * wd) / spacingIn;
  return {
    name: `${size[0]}x${size[1]} ${grade.species} boards ${o.spacing === undefined ? "laid tight" : `@ ${spacingIn.toFixed(1)}" o.c.`} (G ${row.g}: ${pcf.toFixed(1)} pcf)`,
    psf: psfValue,
    thickness: inch(tk),
    strips: o.spacing === undefined ? undefined : { width: inch(wd), spacing: o.spacing, along: "run", sheet: false },
    verified: true,
  };
}

/** A dead load: total pressure, the breakdown for reports, and the layers when built from them. */
export interface DeadLoad {
  readonly pressure: number;
  readonly label: string;
  readonly layers?: readonly Layer[];
}
/**
 * Dead load from material layers, top (weather side) first, looked up in the
 * material-weights table (ASCE 7 Table C3.1-1a and product sheets), e.g.
 * `layers("asphalt shingles", "roofing felt", "osb 7/16")`. A `[key, count]`
 * pair repeats a layer; a `[description, psf]` with a number adds your own
 * value (no thickness); a `Layer` (e.g. from `boards(...)`) is used as is.
 */
export function layers(...items: (string | readonly [string, number] | Layer)[]): DeadLoad {
  const t = dataTable<MaterialWeightRow>("material-weights");
  const out: Layer[] = [];
  const parts: string[] = [];
  for (const it of items) {
    if (typeof it === "object" && !Array.isArray(it)) {
      const l = it as Layer;
      out.push(l);
      parts.push(`${l.name} ${l.psf.toFixed(2)}${l.verified ? "" : "*"}`);
      continue;
    }
    const [key, n] = typeof it === "string" ? [it, 1] : (it as readonly [string, number]);
    const row = t.rows.find((r) => r.key === key);
    if (row) {
      const strips = row.ribs && { width: inch(row.ribs.width_in), spacing: inch(row.ribs.spacing_in), along: row.ribs.along, sheet: true };
      for (let k = 0; k < n; k++) out.push({ name: row.description, psf: row.psf, thickness: inch(row.thickness_in ?? 0), strips, verified: row.verified });
      parts.push(`${n === 1 ? "" : n + " × "}${row.description} ${row.psf}${row.verified ? "" : "*"}`);
    } else if (typeof it !== "string") {
      out.push({ name: key, psf: n, thickness: 0, verified: false });
      parts.push(`${key} ${n.toFixed(2)}`);
    } else {
      throw new Error(`no material "${key}" in ${t.source.publication} ${t.source.table} (known: ${t.rows.map((r) => r.key).join(", ")})`);
    }
  }
  const total = out.reduce((a, l) => a + l.psf, 0);
  const star = parts.some((p) => p.includes("*")) ? " (* unverified)" : "";
  return { pressure: psf(total), label: `${parts.join(" + ")} = ${total.toFixed(1)} psf, ${t.source.publication} ${t.source.edition} ${t.source.table}${star}`, layers: out };
}

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
  No3: grade("DFL", "No.3"),
  Stud: grade("DFL", "Stud"),
  /** Timbers (5" and thicker, NDS Table 4D): beams & stringers (width > thickness + 2"). */
  BeamsStringersSelect: grade("DFL", "B&S Select Structural"),
  BeamsStringersNo1: grade("DFL", "B&S No.1"),
  BeamsStringersNo2: grade("DFL", "B&S No.2"),
  /** Timbers: posts & timbers (width ≤ thickness + 2"). */
  PostsTimbersSelect: grade("DFL", "P&T Select Structural"),
  PostsTimbersNo1: grade("DFL", "P&T No.1"),
  PostsTimbersNo2: grade("DFL", "P&T No.2"),
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

// ----- surveyed walls -------------------------------------------------------

/**
 * An end of a wall: `"start"` / `"end"` (the order the perimeter path
 * visits its corners), or a compass direction — `"south"` is whichever end
 * lies to the south, so you don't have to remember which way the path ran.
 */
export type WallEnd = "start" | "end" | "north" | "south" | "east" | "west";

interface OffsetSpec {
  readonly distance: Length;
  readonly note: string;
}
interface DatumSpec {
  readonly datum: { readonly kind: "corner"; readonly end: WallEnd; readonly face: "inside" | "outside" } | { readonly kind: "member"; readonly name: string; readonly toward: WallEnd };
  readonly offsets: readonly OffsetSpec[];
}

/**
 * Where the tape or laser sits for a reading along a wall. Readings measure
 * away from a corner along the wall (or, from a member, toward the end
 * named). Shift it with `.offset(...)` when it sits on something the model
 * doesn't frame, e.g. drywall:
 *
 *   Datum.corner("south").offset(inch(0.5), "1/2\" drywall on the house wall")
 */
export class Datum {
  private constructor(readonly spec: DatumSpec) {}
  /**
   * A corner of the wall being surveyed. `"inside"` (default): the framing
   * face of the wall met there, from inside the building. `"outside"`: the
   * outside corner of the footprint (the traverse point).
   */
  static corner(end: WallEnd, face: "inside" | "outside" = "inside"): Datum {
    return new Datum({ datum: { kind: "corner", end, face }, offsets: [] });
  }
  /** The face of a named member of this survey on its `toward` side; readings go on toward that end. */
  static member(name: string, toward: WallEnd): Datum {
    return new Datum({ datum: { kind: "member", name, toward }, offsets: [] });
  }
  /**
   * Moves the datum `distance` into the space being measured, e.g. the
   * thickness of drywall or trim the laser sits on. Say what it is: the note
   * is kept with the reading.
   */
  offset(distance: Length, note: string): Datum {
    if (!note.trim()) throw new Error("Datum.offset: say what the offset is (e.g. \"1/2\\\" drywall\")");
    return new Datum({ ...this.spec, offsets: [...this.spec.offsets, { distance, note }] });
  }
  toJSON() {
    return this.spec;
  }
}

/** Which face of the member a reading reaches: nearer the datum (default), its centre, or the far face. */
export type Hit = "near" | "centre" | "far";

interface HeightSpec {
  readonly level: "slab" | "plate_top" | "under_top_plate";
  readonly offsets: readonly OffsetSpec[];
  readonly distance: Length;
}
/**
 * A vertical datum in a wall: `Level.slab` (bottom of the bottom plate) and
 * `Level.plateTop` measure up; `Level.underTopPlate` measures down.
 * `Level.slab.offset(inch(0.75), "floor mat").at(inch(82.5))`.
 */
export class Level {
  private constructor(readonly level: HeightSpec["level"], readonly offsets: readonly OffsetSpec[]) {}
  static readonly slab = new Level("slab", []);
  static readonly plateTop = new Level("plate_top", []);
  static readonly underTopPlate = new Level("under_top_plate", []);
  offset(distance: Length, note: string): Level {
    if (!note.trim()) throw new Error("Level.offset: say what the offset is");
    return new Level(this.level, [...this.offsets, { distance, note }]);
  }
  at(distance: Length): HeightSpec {
    return { level: this.level, offsets: this.offsets, distance };
  }
}
const height = (h: Length | HeightSpec): HeightSpec => (typeof h === "number" ? Level.slab.at(h) : h);

type PostRole = "stud" | "king" | "jack" | "cripple";
type SurveyItemSpec =
  | { readonly item: "post"; readonly role: PostRole; readonly at: { readonly from: DatumSpec; readonly distance: Length; readonly hit: Hit }; readonly plies: number; readonly name?: string }
  | {
      readonly item: "opening";
      readonly label: string;
      readonly kind: "door" | "window";
      readonly head?: HeightSpec;
      readonly sill?: HeightSpec;
      readonly header: readonly [number, number, number];
      readonly header_flush: boolean;
    };
interface SurveySpec {
  readonly items: readonly SurveyItemSpec[];
  readonly checks: readonly { readonly from: DatumSpec; readonly to: DatumSpec; readonly distance: Length; readonly tolerance: Length }[];
}

/** Options for one reading. */
export interface ReadingOptions {
  /** Datum for this reading only (default: the survey's current one). */
  readonly from?: Datum;
  /** Face of the member reached (default: the survey's, normally `"near"`). */
  readonly hit?: Hit;
  /** Plies side by side along the wall, e.g. 2 for a doubled stud. */
  readonly plies?: number;
  /** A name, to measure other members from (`Datum.member`) and in reports. */
  readonly name?: string;
}

/**
 * A wall as built, from field measurements: each stud, king, jack and
 * cripple at an absolute reading from an explicit datum (errors don't
 * accumulate as they would with spacings), and the openings between them.
 * Positions come from the readings, so the list can jump — e.g. half the
 * wall from one corner, then the rest from the other:
 *
 *   Survey.from(Datum.corner("south"))
 *     .stud(inch(0))                         // tight to the corner
 *     .stud(inch(15.25)).stud(inch(31.25))
 *     .king(inch(40)).jack(inch(41.5))
 *     .window("W1", { head: inch(82.5), sill: inch(46.5), header: [2, 2, 8] })
 *     .jack(inch(77.5)).king(inch(79))
 *     .from(Datum.corner("north").offset(inch(0.5), "1/2\" drywall"))
 *     .stud(inch(50)).stud(inch(34)) …
 *     .check(Datum.corner("south"), Datum.corner("north"), inch(229.0625))
 *
 * An opening spans between the posts listed either side of it in the list
 * (after any cripples listed next to it): jacks, then the king. With no jacks the
 * header is carried by the kings through nails alone — that is modelled,
 * and flagged by the checks. Readings are to the face nearer the datum
 * unless `hit` says otherwise.
 */
export class Survey {
  private constructor(readonly datum: Datum, readonly hit: Hit, readonly items: readonly SurveyItemSpec[], readonly checks: SurveySpec["checks"]) {}
  /** Starts a survey whose readings are from `datum`, to the `hit` face (default `"near"`). */
  static from(datum: Datum, o: { hit?: Hit } = {}): Survey {
    return new Survey(datum, o.hit ?? "near", [], []);
  }
  /** Following readings are from `datum` (and to the `hit` face, if given). */
  from(datum: Datum, o: { hit?: Hit } = {}): Survey {
    return new Survey(datum, o.hit ?? this.hit, this.items, this.checks);
  }
  private post(role: PostRole, d: Length, o: ReadingOptions): Survey {
    const item: SurveyItemSpec = { item: "post", role, at: { from: (o.from ?? this.datum).spec, distance: d, hit: o.hit ?? this.hit }, plies: o.plies ?? 1, name: o.name };
    return new Survey(this.datum, this.hit, [...this.items, item], this.checks);
  }
  /** A full-height stud. */
  stud(d: Length, o: ReadingOptions = {}): Survey {
    return this.post("stud", d, o);
  }
  /** A full-height stud beside an opening, carrying the header's end. */
  king(d: Length, o: ReadingOptions = {}): Survey {
    return this.post("king", d, o);
  }
  /** A jack (trimmer) under the header, beside the opening. */
  jack(d: Length, o: ReadingOptions = {}): Survey {
    return this.post("jack", d, o);
  }
  /** A cripple within an opening (under the sill and/or over the header). List it next to its opening. */
  cripple(d: Length, o: ReadingOptions = {}): Survey {
    return this.post("cripple", d, o);
  }
  private opening(kind: "door" | "window", label: string, o: { head?: Length | HeightSpec; sill?: Length | HeightSpec; header?: readonly [number, number, number]; flushHeader?: boolean }): Survey {
    const item: SurveyItemSpec = {
      item: "opening",
      label,
      kind,
      head: o.head === undefined ? undefined : height(o.head),
      sill: o.sill === undefined ? undefined : height(o.sill),
      header: o.header ?? [2, 2, 8],
      header_flush: o.flushHeader ?? false,
    };
    return new Survey(this.datum, this.hit, [...this.items, item], this.checks);
  }
  /**
   * A window between the posts before and after it. `head` (top of the rough
   * opening) and `sill` are from the slab unless given as `Level…at(...)`.
   * `header`: (plies, nominal thickness, nominal depth).
   */
  window(label: string, o: { head: Length | HeightSpec; sill: Length | HeightSpec; header?: readonly [number, number, number] }): Survey {
    return this.opening("window", label, o);
  }
  /** A door between the posts before and after it; `flushHeader` sets the header tight under the top plate. */
  door(label: string, o: { head?: Length | HeightSpec; header?: readonly [number, number, number]; flushHeader?: boolean }): Survey {
    if (o.head === undefined && !o.flushHeader) throw new Error(`door ${label}: give its head height or flushHeader: true`);
    return this.opening("door", label, o);
  }
  /**
   * A tie-out: a distance measured between two datums (e.g. corner to
   * corner), compared with the model and reported; a warning if it differs
   * by more than `tolerance` (default 1/4").
   */
  check(from: Datum, to: Datum, distance: Length, o: { tolerance?: Length } = {}): Survey {
    return new Survey(this.datum, this.hit, this.items, [...this.checks, { from: from.spec, to: to.spec, distance, tolerance: o.tolerance ?? inch(0.25) }]);
  }
  toJSON(): SurveySpec {
    return { items: this.items, checks: this.checks };
  }
}

// ----- walls ---------------------------------------------------------------

interface WallSpec {
  readonly name: string;
  readonly height: Length;
  readonly studs: Nominal;
  readonly spacing: Length;
  readonly layout_from?: readonly Length[];
  readonly grade: Grade;
  readonly header_grade?: Grade;
  readonly justify: "exterior" | "center";
  readonly openings: readonly OpeningSpec[];
  readonly cap_breaks: readonly Length[];
  readonly survey?: SurveySpec;
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
  /**
   * As built: the stud layout is set out from a stud whose centre is `x`
   * along the wall from its start (the others follow at the spacing, either
   * way), not from the wall's start. Further studs set out the stretch
   * between openings each is in, where that is framed on its own (a pier
   * between two doors).
   */
  studsFrom(x: Length, ...stretches: Length[]): Wall {
    return this.with({ layout_from: [x, ...stretches] });
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
  /**
   * Frame this wall exactly as measured (see `Survey`) instead of by layout
   * rules; its openings are part of the survey.
   */
  surveyed(s: Survey): Wall {
    return this.with({ survey: s.toJSON() });
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
  /** One of this perimeter's walls as a drawing target, e.g. `elevation(p.wall("Wall A"))`. */
  wall(name: string): string {
    return `${this.name}/${name}`;
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
export const pointAlong = (a: Point, b: Point, d: number): Point => lerp(a, b, d / Math.hypot(b[0] - a[0], b[1] - a[1]));
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

export type LoadKind = "dead" | "live" | "roof_live" | "snow" | "wind" | "seismic";
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
    readonly overrides?: readonly { readonly trusses: readonly number[]; readonly shape: TrussShape }[];
    readonly shifts?: readonly { readonly trusses: readonly number[]; readonly along: Length }[];
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
  /**
   * A different shape for some trusses (1-based numbers, T1 at the origin),
   * e.g. the trusses over a wing with a different bearing. The same
   * spacing and loads apply.
   */
  withShape(trusses: readonly number[], shape: TrussShape): TrussRoof {
    return new TrussRoof({ ...this.spec, overrides: [...(this.spec.overrides ?? []), { trusses, shape }] });
  }
  /**
   * Trusses as built off the layout (1-based numbers, T1 at the origin):
   * each stands `along` further along the ridge than its place at the
   * spacing (negative: back toward the origin). As a surveyed wall takes its
   * studs from readings, not from the layout rule.
   */
  shifted(trusses: readonly number[], along: Length): TrussRoof {
    return new TrussRoof({ ...this.spec, shifts: [...(this.spec.shifts ?? []), { trusses, along }] });
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
  /**
   * Trusses bear on the cap plates of these perimeters (which also set the
   * height if not given) and on the horizontal members (beams, headers) of
   * these assemblies, wherever a bottom chord crosses them.
   */
  bearingOn(...ps: (Perimeter | Assembly)[]): TrussRoof {
    return new TrussRoof({ ...this.spec, bears_on: [...this.spec.bears_on, ...ps.map((p) => p.name)] });
  }
  /**
   * Uniform load on the roof, in pascals (use `psf`). `basis`: per unit of
   * plan area (default; snow and roof live loads) or of sloped surface.
   */
  load(kind: LoadKind, pressure: number, o: { basis?: "plan" | "surface"; label?: string; on?: "slope" | "flat"; layers?: readonly Layer[] } = {}): TrussRoof {
    return new TrussRoof({ ...this.spec, loads: [...this.spec.loads, { kind, pressure, ...o }] });
  }
  /**
   * Roof snow load on plan area, e.g. from `roofSnow(site, { ce: 1.0, ct: 1.2 })`.
   * `on: "flat"` puts it on the flat roof over the bottom chords beyond the heels.
   */
  snow(d: DeadLoad, o: { on?: "slope" | "flat" } = {}): TrussRoof {
    return this.load("snow", d.pressure, { basis: "plan", label: d.label, ...o });
  }
  /** Roofing dead load along the slope (or on the flat part), from `layers(...)`. */
  dead(d: DeadLoad, o: { on?: "slope" | "flat" } = {}): TrussRoof {
    return this.load("dead", d.pressure, { basis: "surface", label: d.label, layers: d.layers, ...o });
  }
  get name(): string {
    return this.spec.name;
  }
  /** The `i`-th truss (1-based) as a drawing target, e.g. `elevation(roof.truss(2))`. */
  truss(i: number): string {
    return `${this.spec.name}/T${i}`;
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

/**
 * How members that meet are fastened: typical nailing, truss plates, none,
 * or explicit nails, e.g. `{ nails: 4, penny: 16, method: "end" }`.
 */
export type Joints = "nailed" | "truss_plate" | "none" | { readonly nails: number; readonly penny: 6 | 8 | 10 | 12 | 16 | 20; readonly method: "end" | "toe" | "face" };

type AssemblyLoad = { readonly kind: LoadKind; readonly member: string; readonly w: number } | { readonly kind: LoadKind; readonly point: string; readonly force: number };

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
    readonly loads: readonly AssemblyLoad[],
    readonly bonds: readonly (readonly [string, string])[],
  ) {}

  static named(name: string): Assembly {
    return new Assembly(name, Placement.identity, [], [], "nailed", [], [], [], []);
  }
  private copy(
    p: Partial<{ name: string; placement: Placement; points: Assembly["points"]; members: Assembly["members"]; joints: Joints; bearsOn: readonly string[]; supports: readonly string[]; loads: readonly AssemblyLoad[]; bonds: Assembly["bonds"] }>,
  ): Assembly {
    return new Assembly(
      p.name ?? this.name,
      p.placement ?? this.placement,
      p.points ?? this.points,
      p.members ?? this.members,
      p.joints ?? this.joints_,
      p.bearsOn ?? this.bearsOn,
      p.supports ?? this.supports,
      p.loads ?? this.loads,
      p.bonds ?? this.bonds,
    );
  }
  /** Two members fastened face to face (e.g. a cap nailed onto a header): `"header_cap"`, `"header"` or `"post#2"`. */
  bonded(a: string, b: string): Assembly {
    for (const x of [a, b]) if (!this.members.some((m) => m.role === x.split("#")[0])) throw new Error(`assembly ${this.name}: no member ${x}`);
    return this.copy({ bonds: [...this.bonds, [a, b]] });
  }
  /** Uniform downward load along members: `"beam"` (every member with that role) or `"beam#2"`, in N/m (use `plf`). */
  lineLoad(kind: LoadKind, member: string, w: number): Assembly {
    if (!this.members.some((m) => m.role === member.split("#")[0])) throw new Error(`assembly ${this.name}: no member ${member}`);
    return this.copy({ loads: [...this.loads, { kind, member, w }] });
  }
  /** Downward point load at a named point, in N (use `lbf`). */
  pointLoad(kind: LoadKind, point: string, force: number): Assembly {
    if (!this.points.some((p) => p.name === point)) throw new Error(`assembly ${this.name}: unknown point ${point}`);
    return this.copy({ loads: [...this.loads, { kind, point, force }] });
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
      loads: this.loads,
      bonds: this.bonds,
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

// ----- scenarios ---------------------------------------------------------------

interface ScenarioInfo {
  readonly name: string;
  readonly description?: string;
}
const scenarioRegistry: ScenarioInfo[] = [];
let scenarioDefault: string | undefined;

/**
 * Named alternatives, one of which is in effect: returns the value for the
 * scenario being evaluated (chosen in `topo serve`, else `default`, else the
 * first). Use it for anything that differs — loads, code editions, even
 * geometry ("before" / "after"):
 *
 *   const basis = scenarios({
 *     "Current code": { snow: roofSnow(site, …), asce7: "7-16" },
 *     "As built (1979)": { snow: { pressure: psf(25), label: "…" }, combinations: "ubc-1976" },
 *   }, { describe: { "As built (1979)": "1976 UBC as adopted by King County" } });
 */
export function scenarios<T>(choices: Record<string, T>, o: { describe?: Record<string, string>; default?: string } = {}): T {
  const names = Object.keys(choices);
  if (!names.length) throw new Error("scenarios: give at least one choice");
  for (const n of names) if (!scenarioRegistry.some((s) => s.name === n)) scenarioRegistry.push({ name: n, description: o.describe?.[n] });
  scenarioDefault = scenarioDefault ?? o.default ?? names[0];
  return choices[currentScenario() in choices ? currentScenario() : (o.default ?? names[0])];
}

/** The scenario being evaluated. */
export function currentScenario(): string {
  const want = (globalThis as { __topo_scenario?: string }).__topo_scenario;
  return want !== undefined && scenarioRegistry.some((s) => s.name === want) ? want : (scenarioDefault ?? "");
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

/**
 * A direction: a name (`"north"`, `"up"`, a group's `"inside"`/`"span"`/…,
 * or an axis `"+x"`…`"-z"`) or a vector, in the frame of a group or member
 * (`in`, a path tail such as `"Garage"` or `"T2"`; default the world, where
 * +x is east, +y north and +z up).
 */
export interface DirRef {
  readonly name?: string;
  readonly vector?: Point3;
  readonly in?: string;
}
const dirRef = (d: string | Point3, o: { in?: string } = {}): DirRef =>
  typeof d === "string" ? { name: d, ...o } : { vector: d, ...o };

/**
 * A face in the member's canonical frame: x runs with the grain (first node
 * to last), y across the thickness, z across the depth. `±x` are the cut
 * ends, `±y` the wide faces, `±z` the narrow edges.
 */
export type Axis = "+x" | "-x" | "+y" | "-y" | "+z" | "-z";
/** @deprecated aliases: top/bottom = ±z, back/front = ±y, end/start = ±x. */
export type LegacyFaceSide = "top" | "bottom" | "front" | "back" | "start" | "end";
export type FaceSide = Axis | LegacyFaceSide;

/** One plane of a member solid, named through the member's stable path. */
export type PlaneRef =
  | { readonly kind: "face"; readonly member: string; readonly side: FaceSide }
  | { readonly kind: "mid"; readonly member: string; readonly axis: "y" | "z" | "depth" | "width" }
  | { readonly kind: "facing"; readonly member: string; readonly direction: DirRef };
/** Intersection of 1–3 planes: a face, an edge/corner line, or a point. */
export interface Feature {
  readonly planes: readonly PlaneRef[];
}

/** A member, by the `/`-separated tail of its path (e.g. `"T2/bottom_chord.F-H1"`). */
export class MemberRef {
  constructor(readonly path: string) {}
  /** A face by canonical axis, e.g. `face("+z")`. */
  face(side: FaceSide): PlaneRef {
    return { kind: "face", member: this.path, side };
  }
  /** Mid-plane across the member's z (depth) or y (thickness), e.g. a wall centreline. */
  mid(axis: "y" | "z" | "depth" | "width"): PlaneRef {
    return { kind: "mid", member: this.path, axis };
  }
  /** The face whose outward normal is closest to a direction (see `facing`). */
  facing(dir: string | Point3, o: { in?: string } = {}): PlaneRef {
    return facing(this, dir, o);
  }
}
export const member = (path: string): MemberRef => new MemberRef(path);
/**
 * The face of `m` whose outward normal points closest to a direction (within
 * 45°): `facing(member("Wall A/cap_plate"), "up")`, `facing(stud, "inside",
 * { in: "Wall A" })`, `facing(post, "north")`.
 */
export const facing = (m: MemberRef, dir: string | Point3, o: { in?: string } = {}): PlaneRef => ({
  kind: "facing",
  member: m.path,
  direction: dirRef(dir, o),
});
/** Where faces meet: two faces make an edge/corner line, three a point. */
export const meet = (...planes: PlaneRef[]): Feature => ({ planes });
const asFeature = (f: Feature | PlaneRef): Feature => ("planes" in f ? f : { planes: [f] });

export type Quantity =
  | { readonly kind: "horizontal"; readonly a: Feature; readonly b: Feature }
  | { readonly kind: "vertical"; readonly a: Feature; readonly b: Feature }
  | { readonly kind: "length"; readonly member: string; readonly how: "long" | "centreline" }
  | { readonly kind: "along"; readonly a: Feature; readonly b: Feature; readonly direction: DirRef }
  | { readonly kind: "rise"; readonly member: string; readonly run: Length };
export const horizontal = (a: Feature | PlaneRef, b: Feature | PlaneRef): Quantity => ({ kind: "horizontal", a: asFeature(a), b: asFeature(b) });
export const vertical = (a: Feature | PlaneRef, b: Feature | PlaneRef): Quantity => ({ kind: "vertical", a: asFeature(a), b: asFeature(b) });
/** `long`: lumber length (long point to long point); `centreline`: between the end cuts along its centreline. */
export const lengthOf = (m: MemberRef, how: "long" | "centreline"): Quantity => ({ kind: "length", member: m.path, how });
/** Distance between two features along a named direction, e.g. `along(a, b, "north")` or `along(a, b, "span", { in: "T2" })`. */
export const along = (a: Feature | PlaneRef, b: Feature | PlaneRef, dir: string | Point3, o: { in?: string } = {}): Quantity => ({
  kind: "along",
  a: asFeature(a),
  b: asFeature(b),
  direction: dirRef(dir, o),
});
/** Rise of a sloped member over a horizontal `run` (e.g. a level held on a rafter: `riseOver(member("T2/top_chord.L"), inch(12))`). */
export const riseOver = (m: MemberRef, run: Length): Quantity => ({ kind: "rise", member: m.path, run });

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

// ----- drawings: sheets and views as code ----------------------------------

/** What a view draws: a group or member path tail (`"Left wall"`, `"T2"`), an item, or a member. */
export type Target = string | { readonly name: string } | MemberRef;
const targetPath = (t: Target): string => (typeof t === "string" ? t : t instanceof MemberRef ? t.path : t.name);
const targets = (t: Target | readonly Target[] | undefined): string[] =>
  t === undefined ? [] : Array.isArray(t) ? (t as readonly Target[]).map(targetPath) : [targetPath(t as Target)];

export interface ViewOptions {
  readonly title?: string;
  readonly subtitle?: string;
  /** A standard scale: `'1/4"'`, `'1-1/2"'`, `"1:50"`; default the largest that fits. */
  readonly scale?: string;
  /** Direction toward the viewer: a name (`"south"`, a wall's `"outside"`, …) or vector. */
  readonly from?: string | Point3;
  /** Direction drawn up the page. */
  readonly up?: string | Point3;
  /** Frame (group or member path) that `from`/`up` names and vectors are in; default the drawn group. */
  readonly in?: string;
  /** Also drawn, with hidden edges dashed (e.g. walls below a roof plan). */
  readonly dashed?: Target | readonly Target[];
  /** Piece-mark tags (default true). */
  readonly tags?: boolean;
  /** The drawn groups' dimensions and notes that are true length in this view (default true). */
  readonly dims?: boolean;
}

export type ViewKind = "plan" | "elevation" | "iso" | "detail" | "analytical" | "utilization" | "schedule" | "notes";
export interface ViewSpec {
  readonly kind: ViewKind;
  readonly [k: string]: unknown;
}
export type SheetSpec =
  | { readonly kind: "standard"; readonly which: StandardSheets }
  | { readonly kind: "sheet"; readonly number: string; readonly title: string; readonly views: readonly ViewSpec[] };
/** The generated sheet sets, in their default order. */
export type StandardSheets = "cover" | "plans" | "trusses" | "walls" | "analytical" | "schedules";
const STANDARD: readonly StandardSheets[] = ["cover", "plans", "trusses", "walls", "analytical", "schedules"];

function projectedView(kind: ViewKind, of: Target | readonly Target[] | undefined, o: ViewOptions & { radius?: Length; at?: Feature }): ViewSpec {
  const dir = (d: string | Point3 | undefined) => (d === undefined ? undefined : dirRef(d, o.in === undefined ? {} : { in: o.in }));
  return {
    kind,
    title: o.title,
    subtitle: o.subtitle,
    of: targets(of),
    dashed: targets(o.dashed),
    from: dir(o.from),
    up: dir(o.up),
    scale: o.scale,
    tags: o.tags ?? true,
    dims: o.dims ?? true,
    radius: o.radius,
    at: o.at,
  };
}

/** A plan (looking down, building +y up the page) of groups or members; all if omitted. */
export const plan = (of?: Target | readonly Target[], o: ViewOptions = {}): ViewSpec => projectedView("plan", of, o);
/** An elevation, looking from `from` (default: the group's −y, e.g. a wall's outside or a truss's face). */
export const elevation = (of?: Target | readonly Target[], o: ViewOptions = {}): ViewSpec => projectedView("elevation", of, o);
/** An axonometric view (default from the building's south-west, above; untagged unless `tags: true`). */
export const iso = (of?: Target | readonly Target[], o: ViewOptions = {}): ViewSpec => projectedView("iso", of, { tags: false, ...o });
/**
 * An enlarged view around a feature, clipped to a circle of `radius`
 * (default 12"): `detail(meet(bc.facing("up"), top.facing("up")), roof.truss(2), { scale: '1-1/2"' })`.
 */
export const detail = (at: Feature | PlaneRef, of?: Target | readonly Target[], o: ViewOptions & { radius?: Length } = {}): ViewSpec =>
  projectedView("detail", of, { ...o, at: asFeature(at) });
/** The centre-line model with junction symbols. */
export const analytical = (of?: Target | readonly Target[], o: ViewOptions = {}): ViewSpec => projectedView("analytical", of, o);
/**
 * Members filled by their highest demand/capacity ratio over all load
 * combinations (NDS ASD), labelled, with a legend. Roofs and floors are shown
 * in plan, walls and trusses in elevation (override with `from`).
 */
export const utilization = (of?: Target | readonly Target[], o: ViewOptions = {}): ViewSpec => projectedView("utilization", of, o);
/**
 * A table: `"members"`, `"connections"`, `"junctions"`, `"checks"` (the
 * governing check of each member), `"reactions"` (foundation reactions by
 * load case), `"layers"` (roof build-ups from `layers(...)`), or a tool
 * table such as `"Field measurements"`.
 */
export const schedule = (table: string, o: { title?: string } = {}): ViewSpec => ({ kind: "schedule", table, title: o.title });
/** A block of notes. */
export const notes = (title: string, ...lines: string[]): ViewSpec => ({ kind: "notes", title, lines });

/** A sheet (a tab in `topo serve`); views are packed in order, continuing on `S-502`… if needed. */
export const sheet = (number: string, title: string, ...views: (ViewSpec | readonly ViewSpec[])[]): SheetSpec => ({
  kind: "sheet",
  number,
  title,
  views: views.flat() as ViewSpec[],
});
/** Generated sheet sets, by name (all six, in the default order, if none named). */
export const standardSheets = (...which: StandardSheets[]): SheetSpec[] =>
  (which.length ? which : STANDARD).map((w) => ({ kind: "standard", which: w }));

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

/**
 * Where a building sits in the world: its origin, and the compass bearing of
 * its +x axis in degrees clockwise from north (90, the default, is east).
 */
export interface BuildingPlacement {
  readonly origin?: Point3;
  readonly xBearing?: number;
}

export interface DesignBasis {
  readonly asce7?: "7-16" | "7-22";
  /**
   * Load combinations from another code instead of ASCE 7: `"ubc-1976"` (the
   * 1976 Uniform Building Code, as adopted by King County in 1978: loads
   * summed unfactored, with its load-duration increases) — to check a
   * building against the code it was permitted under.
   */
  readonly combinations?: "ubc-1976";
  readonly selfWeight?: boolean;
}

/** A building, in its own frame; a script exports one, or a `Site` of several. */
export class Building {
  private constructor(
    readonly name: string,
    readonly info_: Partial<Info>,
    readonly items: readonly Item[],
    readonly measurements: readonly Measurement[],
    readonly placement_: BuildingPlacement | undefined,
    readonly directions_: Readonly<Record<string, Point3>>,
    readonly sheets_: readonly SheetSpec[] | undefined,
    readonly basis_: DesignBasis,
  ) {}

  static named(name: string): Building {
    return new Building(name, {}, [], [], undefined, {}, undefined, {});
  }
  private with(
    o: Partial<{ info_: Partial<Info>; items: readonly Item[]; measurements: readonly Measurement[]; placement_: BuildingPlacement; directions_: Record<string, Point3>; sheets_: readonly SheetSpec[]; basis_: DesignBasis }>,
  ): Building {
    const v = { ...this, ...o };
    return new Building(this.name, v.info_, v.items, v.measurements, v.placement_, v.directions_, v.sheets_, v.basis_);
  }
  /**
   * Design basis: the ASCE 7 edition the loads come from (`"7-16"`, the
   * default, or `"7-22"` — whose strength-level snow enters ASD combinations
   * at 0.7S), and whether member self-weight is added to the dead load
   * (default true; turn off when the dead load already includes it).
   */
  designBasis(b: DesignBasis): Building {
    return this.with({ basis_: { ...this.basis_, ...b } });
  }
  /**
   * The drawing set, in tab order. Without this, the standard sets are drawn;
   * with it, only what is listed: `.sheets(standardSheets("cover"), sheet("S-501", "Details", …), standardSheets("schedules"))`.
   * Repeated calls append.
   */
  sheets(...s: (SheetSpec | readonly SheetSpec[])[]): Building {
    return this.with({ sheets_: [...(this.sheets_ ?? []), ...(s.flat() as SheetSpec[])] });
  }
  info(i: Partial<Info>): Building {
    return this.with({ info_: { ...this.info_, ...i } });
  }
  /** Add items (arrays, e.g. from `repeat`, are flattened). */
  add(...items: (Item | readonly Item[])[]): Building {
    return this.with({ items: [...this.items, ...items.flat()] });
  }
  /** Field measurements; `unknown`s are fitted to them when the model is run. */
  measure(...ms: (Measurement | readonly Measurement[])[]): Building {
    return this.with({ measurements: [...this.measurements, ...ms.flat()] });
  }
  /** Places the building in the world (+x east, +y north, +z up); items stay in building coordinates. */
  placed(p: BuildingPlacement): Building {
    return this.with({ placement_: { ...this.placement_, ...p } });
  }
  /** Names a direction in the building frame, e.g. `.direction("street", [0, -1, 0])`, usable as `facing(m, "street")`. */
  direction(name: string, v: Point3): Building {
    return this.with({ directions_: { ...this.directions_, [name]: v } });
  }
  toJSON() {
    const p = this.placement_;
    return {
      name: this.name,
      info: this.info_,
      items: this.items,
      measurements: this.measurements,
      unknowns: unknownRegistry,
      placement: p ? { origin: p.origin ?? [0, 0, 0], x_bearing: p.xBearing ?? 90 } : undefined,
      directions: this.directions_,
      sheets: this.sheets_,
      asce7: this.basis_.asce7,
      self_weight: this.basis_.selfWeight,
      combinations: this.basis_.combinations,
      scenarios: scenarioRegistry,
      scenario: scenarioRegistry.length ? currentScenario() : undefined,
    };
  }
}

/** Several buildings placed in one world (e.g. a house and a detached garage). */
export class Site {
  private constructor(
    readonly name: string,
    readonly info_: Partial<Info>,
    readonly buildings: readonly Building[],
    readonly measurements: readonly Measurement[],
    readonly sheets_: readonly SheetSpec[] | undefined = undefined,
  ) {}
  static named(name: string): Site {
    return new Site(name, {}, [], []);
  }
  info(i: Partial<Info>): Site {
    return new Site(this.name, { ...this.info_, ...i }, this.buildings, this.measurements, this.sheets_);
  }
  add(...b: Building[]): Site {
    return new Site(this.name, this.info_, [...this.buildings, ...b], this.measurements, this.sheets_);
  }
  /** Measurements between buildings (each building's own are included too). */
  measure(...ms: (Measurement | readonly Measurement[])[]): Site {
    return new Site(this.name, this.info_, this.buildings, [...this.measurements, ...ms.flat()], this.sheets_);
  }
  /** The site's drawing set (as `Building.sheets`); the buildings' own custom sheets follow. */
  sheets(...s: (SheetSpec | readonly SheetSpec[])[]): Site {
    return new Site(this.name, this.info_, this.buildings, this.measurements, [...(this.sheets_ ?? []), ...(s.flat() as SheetSpec[])]);
  }
  toJSON() {
    return {
      type: "site", name: this.name, info: this.info_, buildings: this.buildings, measurements: this.measurements, unknowns: unknownRegistry, sheets: this.sheets_,
      scenarios: scenarioRegistry, scenario: scenarioRegistry.length ? currentScenario() : undefined,
    };
  }
}
