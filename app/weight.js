// What parts weigh together, as the engine says each (`about.how`: known, about (within so much),
// with_whole, excluded, unknown; `about.kg`, `about.within`, `about.at`): the sum of those that say,
// how far out it may be, where the middle of it is, and how many are not in it and why. The one
// reckoning of it in the page: whatever says a weight says it by this.

/**
 * The total of some parts: { kg, within, at (the middle of the weight, or null), inertia (its moments about
 * that middle, along the model's x, y and z: kg·m², or null), asPoints (how many of those counted say no
 * moments of their own, and are counted as if all their weight were at its middle: the moments are then a
 * least), counted, withWhole, excluded, unknown, of }.
 */
export function tally(parts) {
  const t = { kg: 0, within: 0, at: null, inertia: null, asPoints: 0, counted: 0, withWhole: 0, excluded: 0, unknown: 0, of: parts.length };
  const sum = [0, 0, 0], second = [0, 0, 0];
  for (const m of parts) {
    const a = m.about ?? {};
    if ((a.how === "known" || a.how === "about") && a.kg > 0) {
      t.kg += a.kg; t.within += a.within || 0; t.counted++;
      if (a.at) for (let k = 0; k < 3; k++) sum[k] += a.kg * a.at[k];
      // (About the origin: its own about its middle, and what its being away adds: m (r² - x²) about x.)
      const r = a.at ?? [0, 0, 0], r2 = r[0] * r[0] + r[1] * r[1] + r[2] * r[2];
      if (!a.inertia) t.asPoints++;
      for (let k = 0; k < 3; k++) second[k] += (a.inertia?.[k] ?? 0) + a.kg * (r2 - r[k] * r[k]);
    } else if (a.how === "with_whole") t.withWhole++;
    else if (a.how === "excluded") t.excluded++;
    else t.unknown++;
  }
  if (t.kg > 0) {
    t.at = sum.map((x) => x / t.kg);
    const c2 = t.at[0] * t.at[0] + t.at[1] * t.at[1] + t.at[2] * t.at[2];
    t.inertia = second.map((x, k) => Math.max(0, x - t.kg * (c2 - t.at[k] * t.at[k])));
  }
  return t;
}

/** A weight as it is said: grams under a kilogram; pounds where the model is not metric. */
export const kgSaid = (x, metric) => (metric === false ? `${(x * 2.20462).toFixed(2)} lb` : x < 1 ? `${x < 0.01 ? (x * 1000).toFixed(1) : Math.round(x * 1000)} g` : `${x.toFixed(2)} kg`);

/** A total as it is said: what it comes to, and how far out it may be; "—" where nothing of it says. */
export function weightSaid(t, metric) {
  if (!(t.kg > 0)) return t.excluded && !t.unknown ? "not counted" : "—";
  return `${t.within > 0 ? "about " : ""}${kgSaid(t.kg, metric)}${t.within > 0 ? ` (± ${kgSaid(t.within, metric)})` : ""}`;
}

/** What of a total is not in it, as a sentence or two; nothing, where all of it is counted. */
export function notInIt(t) {
  const n = (k, one, many) => `${k} ${k === 1 ? one : many}`, out = [];
  if (t.unknown) out.push(`${n(t.unknown, "says", "say")} nothing of what ${t.unknown === 1 ? "it weighs" : "they weigh"}, and ${t.unknown === 1 ? "is" : "are"} not in it`);
  if (t.excluded) out.push(`${n(t.excluded, "is", "are")} left out on purpose`);
  if (t.withWhole) out.push(`${n(t.withWhole, "is a piece", "are pieces")} of a part whose weight is said of the whole`);
  return out.length ? `Of its ${t.of} parts: ${out.join("; ")}.` : "";
}

/** Moments of inertia as they are said: about x, y and z, in a unit that suits their size. */
export function inertiaSaid(i, metric) {
  if (!i) return "—";
  const most = Math.max(...i);
  const [k, unit, places] = metric === false ? [3417.17, "lb·in²", 2] : most < 1e-5 ? [1e7, "g·cm²", 1] : [1e4, "kg·cm²", 2];
  return `${i.map((x) => (x * k).toFixed(places)).join(", ")} ${unit}`;
}
