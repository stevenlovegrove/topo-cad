// A site's design criteria (examples/site-template.ts), on the part core: a template to fill in for your jurisdiction.
// Places with rules of their own are in `topo-part/sites` (unincorporated King County, WA, with the county's own roof
// snow rule: `kingCounty`, `kingCountyRoofSnow`).
//
// How to fill it in (by hand, or ask your LLM helper to research it):
//   1. Find your county or city building department's "Climatic and Geographic Design Criteria": IRC Table R301.2(1).
//      It gives the ground snow load, wind speed, seismic design category and frost depth, and is what the permit
//      reviewer checks against.
//   2. Check which ASCE 7 edition those values follow (IRC 2021 → ASCE 7-16; IRC 2024 → ASCE 7-22). ASCE 7-22's ground
//      snow loads are strength-level and by risk category (ascehazardtool.org).
//   3. Seismic values: the USGS design-maps service (free, no key), e.g.
//      https://earthquake.usgs.gov/ws/designmaps/asce7-22.json?latitude=..&longitude=..&riskCategory=II&siteClass=Default&title=site
//   4. Put the document or URL in every `source`. Unsourced values should not go to a permit.
//
// Then, in a model: `import { site } from "./site-template";` and a roof's snow, `roofSnow(site, { ce: 1.0, ct: 1.2 })`
// (an unheated garage: C_t = 1.2), as an `areaLoad` on it.

import { psf, mph, ft, type SiteHazards } from "topo-part";

export const site: SiteHazards = {
  jurisdiction: "YOUR COUNTY, STATE",
  asce7: "7-16",
  riskCategory: 2,
  groundSnow: { value: psf(25), source: "TODO: county IRC Table R301.2(1), URL" },
  windSpeed: { value: mph(115), source: "TODO: county IRC Table R301.2(1), URL" },
  exposure: { value: "B", source: "TODO: site survey (B: suburban; C: open terrain)" },
  seismic: { value: { sds: 0.0, sd1: 0.0, sdc: "B" }, source: "TODO: USGS design maps (lat, lon, site class)" },
  frostDepth: { value: ft(2.5), source: "TODO: county IRC Table R301.2(1)" },
  notes: ["Template values: replace every TODO before relying on them."],
};
