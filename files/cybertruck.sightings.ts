// What is seen of the Cybertruck (examples/cybertruck.ts) in its two photos: each a reading that a thing of the truck is
// at a pixel of a photo, to within so many pixels. These are the truck's important edges, each by what it is (a plate's
// edge on its outer face: `side R/lower edge`; the rocker's lower edge, `rocker R/foot`), and the wheels' hubs, by the
// middle of each wheel's outer face (`axle rear/hub R`). Each edge was found in the photo (at 960 wide) to a fraction of
// a pixel, along its length every few pixels, and a line fitted through what was found (each scatters about it by less
// than 0.6 px); three points of each line are kept. A hub is the middle of its wheel's ring of nuts. (As the present
// pipeline's: old-pipeline, examples/cybertruck.sightings.ts.)
export type Seen = { photo: "side" | "bed"; name: string; pixel: [number, number]; within: number; edge: string };

export const sightings: Seen[] = [
  { photo: "side", name: "body bottom #1", pixel: [376.51, 456.64], within: 1, edge: "side R/lower edge" }, // 1: Claude, 2026-10-14
  { photo: "side", name: "body bottom #2", pixel: [526.52, 457.63], within: 1, edge: "side R/lower edge" }, // 2: Claude, 2026-10-14
  { photo: "side", name: "body bottom #3", pixel: [676.53, 458.63], within: 1, edge: "side R/lower edge" }, // 3: Claude, 2026-10-14
  { photo: "side", name: "rear arch rear slope #4", pixel: [146.33, 373.8], within: 1, edge: "side R/rear arch rear slope" }, // 4: Claude, 2026-10-14
  { photo: "side", name: "rear arch rear slope #5", pixel: [153.08, 364.25], within: 1, edge: "side R/rear arch rear slope" }, // 5: Claude, 2026-10-14
  { photo: "side", name: "rear arch rear slope #6", pixel: [159.82, 354.7], within: 1, edge: "side R/rear arch rear slope" }, // 6: Claude, 2026-10-14
  { photo: "side", name: "rear arch top #7", pixel: [178.41, 350.89], within: 1, edge: "side R/rear arch top" }, // 7: Claude, 2026-10-14
  { photo: "side", name: "rear arch top #8", pixel: [220.0, 352.08], within: 1, edge: "side R/rear arch top" }, // 8: Claude, 2026-10-14
  { photo: "side", name: "rear arch top #9", pixel: [261.59, 353.27], within: 1, edge: "side R/rear arch top" }, // 9: Claude, 2026-10-14
  { photo: "side", name: "rear arch front slope #10", pixel: [282.39, 357.2], within: 1, edge: "side R/rear arch front slope" }, // 10: Claude, 2026-10-14
  { photo: "side", name: "rear arch front slope #11", pixel: [289.89, 367.35], within: 1, edge: "side R/rear arch front slope" }, // 11: Claude, 2026-10-14
  { photo: "side", name: "rear arch front slope #12", pixel: [297.4, 377.49], within: 1, edge: "side R/rear arch front slope" }, // 12: Claude, 2026-10-14
  { photo: "side", name: "front arch rear slope #13", pixel: [759.45, 379.34], within: 1, edge: "side R/front arch rear slope" }, // 13: Claude, 2026-10-14
  { photo: "side", name: "front arch rear slope #14", pixel: [766.2, 369.4], within: 1, edge: "side R/front arch rear slope" }, // 14: Claude, 2026-10-14
  { photo: "side", name: "front arch rear slope #15", pixel: [772.95, 359.47], within: 1, edge: "side R/front arch rear slope" }, // 15: Claude, 2026-10-14
  { photo: "side", name: "front arch top #16", pixel: [793.51, 357.4], within: 1, edge: "side R/front arch top" }, // 16: Claude, 2026-10-14
  { photo: "side", name: "front arch top #17", pixel: [831.52, 359.06], within: 1, edge: "side R/front arch top" }, // 17: Claude, 2026-10-14
  { photo: "side", name: "front arch top #18", pixel: [869.54, 360.71], within: 1, edge: "side R/front arch top" }, // 18: Claude, 2026-10-14
  { photo: "side", name: "front arch front slope #19", pixel: [887.09, 372.12], within: 1, edge: "side R/front arch front slope" }, // 19: Claude, 2026-10-14
  { photo: "side", name: "front arch front slope #20", pixel: [891.53, 379.85], within: 1, edge: "side R/front arch front slope" }, // 20: Claude, 2026-10-14
  { photo: "side", name: "front arch front slope #21", pixel: [895.98, 387.58], within: 1, edge: "side R/front arch front slope" }, // 21: Claude, 2026-10-14
  { photo: "side", name: "beltline crease #22", pixel: [162.77, 312.59], within: 1, edge: "side R/beltline" }, // 22: Claude, 2026-10-14
  { photo: "side", name: "beltline crease #23", pixel: [401.54, 323.99], within: 1, edge: "side R/beltline" }, // 23: Claude, 2026-10-14
  { photo: "side", name: "beltline crease #24", pixel: [640.32, 335.39], within: 1, edge: "side R/beltline" }, // 24: Claude, 2026-10-14
  { photo: "side", name: "roof line behind the cab #25", pixel: [202.32, 280.08], within: 1, edge: "window R/roof behind the cab" }, // 25: Claude, 2026-10-14
  { photo: "side", name: "roof line behind the cab #26", pixel: [277.67, 270.41], within: 1, edge: "window R/roof behind the cab" }, // 26: Claude, 2026-10-14
  { photo: "side", name: "roof line behind the cab #27", pixel: [353.01, 260.74], within: 1, edge: "window R/roof behind the cab" }, // 27: Claude, 2026-10-14
  { photo: "side", name: "roof line over the cab #28", pixel: [403.77, 255.51], within: 1, edge: "window R/roof over the cab" }, // 28: Claude, 2026-10-14
  { photo: "side", name: "roof line over the cab #29", pixel: [475.79, 247.12], within: 1, edge: "window R/roof over the cab" }, // 29: Claude, 2026-10-14
  { photo: "side", name: "roof line over the cab #30", pixel: [547.81, 238.73], within: 1, edge: "window R/roof over the cab" }, // 30: Claude, 2026-10-14
  { photo: "side", name: "A-pillar #31", pixel: [603.03, 254.12], within: 1, edge: "window R/A-pillar" }, // 31: Claude, 2026-10-14
  { photo: "side", name: "A-pillar #32", pixel: [674.56, 276.53], within: 1, edge: "window R/A-pillar" }, // 32: Claude, 2026-10-14
  { photo: "side", name: "A-pillar #33", pixel: [746.08, 298.94], within: 1, edge: "window R/A-pillar" }, // 33: Claude, 2026-10-14
  { photo: "side", name: "hood edge at the chamfer #34", pixel: [900.92, 343.25], within: 1, edge: "hood/edge R over the chamfer" }, // 34: Claude, 2026-10-14
  { photo: "side", name: "hood edge at the chamfer #35", pixel: [908.05, 345.85], within: 1, edge: "hood/edge R over the chamfer" }, // 35: Claude, 2026-10-14
  { photo: "side", name: "hood edge at the chamfer #36", pixel: [915.18, 348.46], within: 1, edge: "hood/edge R over the chamfer" }, // 36: Claude, 2026-10-14
  { photo: "side", name: "light bar end #37", pixel: [920.83, 369.08], within: 1, edge: "chamfer R/front edge" }, // 37: Claude, 2026-10-14
  { photo: "side", name: "light bar end #38", pixel: [921.3, 361.49], within: 1, edge: "chamfer R/front edge" }, // 38: Claude, 2026-10-14
  { photo: "side", name: "light bar end #39", pixel: [921.78, 353.89], within: 1, edge: "chamfer R/front edge" }, // 39: Claude, 2026-10-14
  { photo: "bed", name: "left sail top #40", pixel: [189.18, 80.2], within: 1, edge: "window L/roof behind the cab" }, // 40: Claude, 2026-10-14
  { photo: "bed", name: "left sail top #41", pixel: [197.1, 112.23], within: 1, edge: "window L/roof behind the cab" }, // 41: Claude, 2026-10-14
  { photo: "bed", name: "left sail top #42", pixel: [205.01, 144.25], within: 1, edge: "window L/roof behind the cab" }, // 42: Claude, 2026-10-14
  { photo: "bed", name: "right sail top #43", pixel: [610.97, 103.41], within: 1, edge: "window R/roof behind the cab" }, // 43: Claude, 2026-10-14
  { photo: "bed", name: "right sail top #44", pixel: [698.07, 127.16], within: 1, edge: "window R/roof behind the cab" }, // 44: Claude, 2026-10-14
  { photo: "bed", name: "right sail top #45", pixel: [785.16, 150.92], within: 1, edge: "window R/roof behind the cab" }, // 45: Claude, 2026-10-14
  { photo: "bed", name: "left rear corner #46", pixel: [208.3, 185.72], within: 1, edge: "side L/back edge" }, // 46: Claude, 2026-10-14
  { photo: "bed", name: "left rear corner #47", pixel: [211.02, 256.49], within: 1, edge: "side L/back edge" }, // 47: Claude, 2026-10-14
  { photo: "bed", name: "left rear corner #48", pixel: [213.74, 327.27], within: 1, edge: "side L/back edge" }, // 48: Claude, 2026-10-14
  { photo: "bed", name: "right rear corner #49", pixel: [802.9, 292.8], within: 1, edge: "side R/back edge" }, // 49: Claude, 2026-10-14
  { photo: "bed", name: "right rear corner #50", pixel: [808.84, 245.35], within: 1, edge: "side R/back edge" }, // 50: Claude, 2026-10-14
  { photo: "bed", name: "right rear corner #51", pixel: [814.77, 197.89], within: 1, edge: "side R/back edge" }, // 51: Claude, 2026-10-14
  { photo: "side", name: "rear hub #52", pixel: [223.85, 472.7], within: 1.5, edge: "axle rear/hub R" }, // 52: Claude, 2026-10-14
  { photo: "side", name: "front hub #53", pixel: [826.68, 472.79], within: 1.5, edge: "axle front/hub R" }, // 53: Claude, 2026-10-14
  { photo: "side", name: "rocker bottom #54", pixel: [420.16, 485.46], within: 1, edge: "rocker R/foot" }, // 54: Claude, 2026-10-15
  { photo: "side", name: "rocker bottom #55", pixel: [504.39, 486.04], within: 1, edge: "rocker R/foot" }, // 55: Claude, 2026-10-15
  { photo: "side", name: "rocker bottom #56", pixel: [588.62, 486.63], within: 1, edge: "rocker R/foot" }, // 56: Claude, 2026-10-15
];
