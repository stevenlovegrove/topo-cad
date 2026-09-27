// Site design criteria — a template to fill in for your jurisdiction.
//
// How to fill it in (by hand, or ask your LLM helper to research it):
//   1. Find your county or city building department's "Climatic and
//      Geographic Design Criteria" — IRC Table R301.2(1). It gives ground
//      snow load, wind speed, seismic design category and frost depth, and is
//      what the permit reviewer will check against. Examples:
//        Prince George's County, MD; CRCOG (Connecticut); Riverton, WY.
//   2. Check which ASCE 7 edition those values follow (IRC 2021 → ASCE 7-16;
//      IRC 2024 → ASCE 7-22). ASCE 7-22 ground snow loads are strength-level
//      and risk-category specific (ascehazardtool.org).
//   3. Seismic values: the USGS design-maps service (free, no key), e.g.
//      https://earthquake.usgs.gov/ws/designmaps/asce7-22.json?latitude=..&longitude=..&riskCategory=II&siteClass=Default&title=site
//   4. Put the document or URL in every `source`. Unsourced values should not
//      go to a permit.
//
// Then, in a model:  import { site } from "./site";
//   roof.snow(roofSnow(site, { ce: 1.0, ct: 1.2 }))   // unheated garage: C_t = 1.2
import type { SiteHazards } from "topo-cad";
import { ft } from "topo-cad";

export const site: SiteHazards = {
  jurisdiction: "YOUR COUNTY, STATE",
  asce7: "7-16",
  riskCategory: 2,
  groundSnow: { value: 25, source: "TODO: county IRC Table R301.2(1), URL" },
  windSpeed: { value: 115, source: "TODO: county IRC Table R301.2(1), URL" },
  exposure: { value: "B", source: "TODO: site survey (B: suburban; C: open terrain)" },
  seismic: { value: { sds: 0.0, sd1: 0.0, sdc: "B" }, source: "TODO: USGS design maps (lat, lon, site class)" },
  frostDepth: { value: ft(2.5), source: "TODO: county IRC Table R301.2(1)" },
  notes: ["Template values: replace every TODO before relying on them."],
};
