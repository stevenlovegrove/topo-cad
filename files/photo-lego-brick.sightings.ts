// What is seen of the brick in its photo (as examples/photo-lego-brick.sightings.ts, which the workspace wrote): each a
// thing of the brick at a pixel, to within so many pixels. A corner of its body (by the edges of its plan that meet
// there, at its top or its underside), an edge of its body at its top, or the rim of a stud's top (by the stud).
export type Seen = { name: string; pixel: [number, number]; within: number; what: { kind: "corner" | "edge" | "rim"; id: string } };

export const sightings: Seen[] = [
  { name: "corner of body #14", pixel: [696.01028, 223.29831], within: 3, what: { kind: "corner", id: "e2/e3.top" } },
  { name: "corner of body #15", pixel: [719.57857, 443.82251], within: 3, what: { kind: "corner", id: "e1/e2.top" } },
  { name: "corner of body #16", pixel: [666.63211, 533.15482], within: 3, what: { kind: "corner", id: "e1/e2.under" } },
  { name: "corner of body #17", pixel: [84.36271, 354.91644], within: 3, what: { kind: "corner", id: "e4/e1.top" } },
  { name: "corner of body #18", pixel: [181.60131, 158.21162], within: 3, what: { kind: "corner", id: "e3/e4.top" } },
  { name: "corner of body #19", pixel: [124.49298, 450.7743], within: 3, what: { kind: "corner", id: "e4/e1.under" } },
  { name: "rim of front 3 #20", pixel: [492.96983, 292.88347], within: 3, what: { kind: "rim", id: "front 3" } },
  { name: "rim of front 3 #21", pixel: [477.11747, 360.09685], within: 3, what: { kind: "rim", id: "front 3" } },
  { name: "rim of back 3 #22", pixel: [497.35201, 189.50535], within: 3, what: { kind: "rim", id: "back 3" } },
  { name: "rim of back 2 #23", pixel: [359.45522, 172.81717], within: 3, what: { kind: "rim", id: "back 2" } },
  { name: "rim of back 2 #24", pixel: [395.93111, 203.88156], within: 3, what: { kind: "rim", id: "back 2" } },
  { name: "rim of back 2 #25", pixel: [367.01484, 226.02865], within: 3, what: { kind: "rim", id: "back 2" } },
  { name: "rim of back 1 #26", pixel: [255.18421, 191.76892], within: 3, what: { kind: "rim", id: "back 1" } },
  { name: "rim of back 1 #27", pixel: [184.15636, 174.78421], within: 3, what: { kind: "rim", id: "back 1" } },
  { name: "rim of front 1 #28", pixel: [133.39186, 274.38172], within: 3, what: { kind: "rim", id: "front 1" } },
  { name: "rim of front 1 #29", pixel: [214.01054, 293.35767], within: 3, what: { kind: "rim", id: "front 1" } },
  { name: "rim of front 4 #30", pixel: [696.23861, 360.60135], within: 3, what: { kind: "rim", id: "front 4" } },
  { name: "rim of front 4 #31", pixel: [600.47776, 337.48645], within: 3, what: { kind: "rim", id: "front 4" } },
  { name: "rim of front 3 #32", pixel: [531.4738, 334.73829], within: 3, what: { kind: "rim", id: "front 3" } },
  { name: "edge of body #33", pixel: [93.41595, 331.86927], within: 3, what: { kind: "edge", id: "e4.top" } },
  { name: "edge of body #34", pixel: [151.2247, 366.46169], within: 3, what: { kind: "edge", id: "e1.top" } },
  { name: "edge of body #35", pixel: [323.09468, 389.2513], within: 3, what: { kind: "edge", id: "e1.top" } },
  { name: "edge of body #36", pixel: [526.09008, 420.68343], within: 3, what: { kind: "edge", id: "e1.top" } },
  { name: "rim of back 4 #37", pixel: [628.60143, 259.69856], within: 3, what: { kind: "rim", id: "back 4" } },
  { name: "edge of body #38", pixel: [566.53781, 203.95116], within: 3, what: { kind: "edge", id: "e3.top" } },
  { name: "edge of body #39", pixel: [436.40712, 188.38409], within: 3, what: { kind: "edge", id: "e3.top" } },
  { name: "edge of body #40", pixel: [297.5488, 171.53023], within: 3, what: { kind: "edge", id: "e3.top" } },
  { name: "rim of back 1 #41", pixel: [229.63235, 156.79297], within: 3, what: { kind: "rim", id: "back 1" } },
  { name: "rim of front 2 #42", pixel: [283.89804, 323.62166], within: 3, what: { kind: "rim", id: "front 2" } },
  { name: "rim of front 2 #43", pixel: [318.58908, 342.80176], within: 3, what: { kind: "rim", id: "front 2" } },
  { name: "edge of body #44", pixel: [702.89789, 278.59303], within: 3, what: { kind: "edge", id: "e2.top" } },
  { name: "rim of front 2 #45", pixel: [332.31015, 342.07721], within: 3, what: { kind: "rim", id: "front 2" } },
];
