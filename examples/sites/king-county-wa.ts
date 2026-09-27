// Design criteria for unincorporated King County, Washington.
//
// Transcribed from primary sources (read 2026-09-27):
//  [KCC]  King County Code Title 16 (updated June 5, 2026), §16.05.040
//         "Climatic and Geographical Design Criteria for King County" —
//         replaces IRC Table R301.2(1). https://aqua.kingcounty.gov/council/clerk/code/19_Title_16.pdf
//         §16.04.410: snow loads per the public rule below; minimum roof snow 25 psf.
//  [RULE] King County Public Rule 16-04-500…509 "Structural Loading: Minimum
//         Roof Snow Loads" (effective Oct 22, 2001): P_g = C_g·h_msl, Table 16-V
//         (C_g by location), P_f = C_e·I·P_g (sites ≤ 1,000 ft), Tables 16-W, 16-X.
//         https://cdn.kingcounty.gov/-/media/king-county/depts/local-services/permits/public-rules/16-04snowload.pdf
//  [RBP]  King County DLS "Residential Basics Program" (April 2026): registered
//         plans use SDC D2, 110 mph, Exposure C, roof snow 25 psf minimum.
//         https://cdn.kingcounty.gov/-/media/king-county/depts/local-services/permits/building-land-use-permits/r/residential-basics-program.pdf
//  [SBCC] Washington State Building Code Council: 2021 codes (IRC 2021 → ASCE
//         7-16) effective statewide March 15, 2024; 2024 codes from May 3, 2027.
//         https://sbcc.wa.gov/news/revised-effective-date-2021-codes-march-15-2024
//
// Not covered here (site specific; get them for your parcel):
//  - C_g for locations between the Table 16-V towns: interpolate the isolines on
//    the King County Ground Snow Load Map (King County iMap / GIS).
//  - Wind exposure (B/C/D) and topographic factor K_zt: site specific (IRC
//    R301.2.1.4, R301.2.1.5). King County's registered plans assume Exposure C.
//  - Seismic S_DS / S_D1: USGS design-maps service for the parcel's lat/lon.
//  - Flood hazard: KCC chapter 21A.24.
import type { DeadLoad, SiteHazards } from "topo-cad";
import { inch, psf } from "topo-cad";

const KCC = "King County Code §16.05.040 (Table R301.2(1) for King County), Title 16 updated 2026-06-05";
const RULE = "King County Public Rule 16-04-506 (Minimum Roof Snow Loads, eff. 2001-10-22)";

/** [RULE] Table 16-V: ground snow load coefficient C_g (psf per foot of elevation). */
export const groundSnowCoefficient = {
  "Auburn": 0.05,
  "Bellevue": 0.05,
  "Bothell": 0.05,
  "Black Diamond": 0.05,
  "Carnation": 0.057,
  "Duvall": 0.056,
  "Enumclaw": 0.05,
  "Fall City": 0.073,
  "Issaquah": 0.054,
  "Kent": 0.05,
  "Kirkland": 0.05,
  "Lester": 0.072,
  "North Bend": 0.075,
  "Palmer": 0.063,
  "Renton": 0.05,
  "Seattle": 0.05,
  "Skykomish": 0.094,
  "Snoqualmie Pass": 0.144,
  "Stevens Pass Ski Area": 0.1,
  "Vashon Island": 0.05,
} as const;
export type KingCountyLocation = keyof typeof groundSnowCoefficient;

/** [RULE] Table 16-W: snow exposure coefficient C_e. */
export const snowExposure = {
  /** Generally open terrain and roof slope ≥ 3:12. */
  openTerrainSteepRoof: 0.8,
  /** All other structures. */
  other: 1.0,
} as const;

/** [RULE] Table 16-X: occupancy importance factor I (snow). */
export const snowImportance = {
  essential: 1.15,
  assemblyOver300: 1.15,
  /** Agricultural buildings, production greenhouses and other miscellaneous structures. */
  agriculturalOrMiscellaneous: 0.9,
  allOthers: 1.0,
} as const;

/** [KCC] footnote 3: frost line depth by site elevation (never less than 12"). */
export function frostDepth(elevationFt: number): number {
  return elevationFt <= 1000 ? inch(12) : elevationFt <= 2000 ? inch(18) : inch(24);
}

export interface KingCountyParcel {
  /** Nearest Table 16-V location, or a C_g read from the county map. */
  readonly location: KingCountyLocation | { readonly cg: number; readonly source: string };
  /** Site elevation, feet above sea level. */
  readonly elevationFt: number;
  /**
   * [KCC] footnote 2: `"D1"` east of the Snoqualmie River (county line to
   * Snoqualmie), east of Snoqualmie, east of the Snoqualmie Parkway and Echo
   * Lake–Snoqualmie Cut-off SE to SR 18, and south/east of SR 18; `"D2"`
   * everywhere else in unincorporated King County.
   */
  readonly seismicDesignCategory: "D1" | "D2";
  /** Wind exposure for the site (B suburban/wooded, C open); site specific. */
  readonly exposure: "B" | "C" | "D";
  readonly exposureSource?: string;
  /** S_DS / S_D1 for the parcel (e.g. from the USGS design-maps service). */
  readonly seismicValues?: { readonly sds: number; readonly sd1?: number; readonly source: string };
}

/** Site hazards for a parcel in unincorporated King County. */
export function kingCounty(p: KingCountyParcel): SiteHazards {
  const [cg, cgSource] = typeof p.location === "string" ? [groundSnowCoefficient[p.location], `${RULE}, Table 16-V (${p.location})`] : [p.location.cg, p.location.source];
  const pg = cg * p.elevationFt;
  return {
    jurisdiction: "Unincorporated King County, WA",
    asce7: "7-16",
    riskCategory: 2,
    groundSnow: { value: pg, source: `${RULE}: P_g = C_g·h = ${cg} × ${p.elevationFt} ft = ${pg.toFixed(1)} psf (C_g: ${cgSource})` },
    windSpeed: { value: 110, source: `${KCC} (ultimate design wind speed)` },
    exposure: { value: p.exposure, source: p.exposureSource ?? "site specific (IRC R301.2.1.4); King County registered plans assume C" },
    seismic: {
      value: { sdc: p.seismicDesignCategory, sds: p.seismicValues?.sds, sd1: p.seismicValues?.sd1 },
      source: `SDC: ${KCC}, footnote 2; ` + (p.seismicValues ? `S_DS/S_D1: ${p.seismicValues.source}` : "S_DS, S_D1: look up the parcel with the USGS design-maps service"),
    },
    frostDepth: { value: frostDepth(p.elevationFt), source: `${KCC}, footnote 3` },
    notes: [
      "Weathering: moderate. Termite: slight to moderate. Decay: slight to moderate. Winter design temperature 25 °F. Ice shield: not required. Air freezing index 100–250. Mean annual temperature 50 °F. (KCC §16.05.040)",
      "Codes: Washington State 2021 IRC/IBC (ASCE 7-16) statewide since 2024-03-15; 2024 codes from 2027-05-03 (SBCC).",
      ...(p.elevationFt > 500 ? ["Sites over 500 ft need additional approval under King County's Residential Basics Program."] : []),
    ],
  };
}

/**
 * [RULE] Roof snow load for a King County site at or below 1,000 ft:
 * P_f = C_e·I·P_g, and never less than 25 psf (KCC §16.04.410). Above
 * 1,000 ft, drift and sliding snow must be considered and the roof load
 * determined by SEAW "Snow Load Analysis for Washington" or ASCE 7 — this
 * helper refuses rather than under-estimate.
 */
export function kingCountyRoofSnow(site: SiteHazards, o: { elevationFt: number; ce: number; importance: number }): DeadLoad {
  if (!site.groundSnow) throw new Error("no ground snow load");
  if (o.elevationFt > 1000) throw new Error(`site at ${o.elevationFt} ft: above 1,000 ft King County requires a full snow analysis (drift, sliding) — ${RULE} §C`);
  const pg = site.groundSnow.value;
  const pf = o.ce * o.importance * pg;
  const design = Math.max(25, pf);
  return {
    pressure: psf(design),
    label:
      `Roof snow P_f = C_e·I·P_g = ${o.ce} × ${o.importance} × ${pg.toFixed(1)} = ${pf.toFixed(1)} psf` +
      (design > pf ? `; King County minimum 25 psf governs` : "") +
      ` (${RULE}; KCC §16.04.410). Ground snow: ${site.groundSnow.source}`,
  };
}
