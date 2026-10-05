// A kids' pedal cart built from 40 x 40 T-slot aluminium profile, in the
// manner of a build-your-own ride kit: profiles cut to length and joined
// with catalog connectors. Not a building — millimetres, a 3D frame
// analysis, and load cases that are things a child does to it.
//
// Choose how the profiles are joined in the Loads panel (Scenario). As drawn,
// the checks say what a kit designer would want to know: the profiles are
// far from their limit, but a child heaving back on the handles puts more
// moment into the backrest's and steering column's brackets than their maker
// rates them for (a brace, or a second pair of brackets, is the fix).
import { Assembly, Connector, deflection, elevation, iso, kgf, mm, Model, plan, scenarios, schedule, sheet, tslot, utilization, type Joints } from "topo-cad";

// ----- dimensions (x forward, y left, z up) -------------------------------------
const profile = tslot("40x40"); // item Profile 8 40x40 light, in its maker's alloy
const wheelbase = mm(800);
const rearTrack = mm(560);
const frontTrack = mm(440);
const axleHeight = mm(120);
const seatAt = mm(230); // along the spine from the rear beam
const seatHeight = mm(200);
const seatWidth = mm(300);
const backrestHeight = mm(420);
const columnFoot = mm(560);
const columnTop: readonly [number, number] = [mm(470), mm(380)]; // x, height above the spine
const handleWidth = mm(320);
const z = axleHeight;

// ----- how it is joined ------------------------------------------------------------
const joint = scenarios<Joints>(
  {
    "Angle brackets, two per joint": Connector.angleBracket({ count: 2 }),
    "One angle bracket per joint": Connector.angleBracket({ count: 1 }),
    "Screw into the core bore": Connector.endFastener(),
  },
  {
    describe: {
      "Angle brackets, two per joint": "Die-cast brackets either side of each joint: rated 2 x (50 N·m, 1000 N)",
      "One angle bracket per joint": "A single bracket: rated 50 N·m and 1000 N",
      "Screw into the core bore": "No capacity is published for this fastener, so the joints are not checked",
    },
  },
);

// ----- loads -------------------------------------------------------------------------
const child = kgf(37); // the heaviest rider of a ride for ages 4–8
const heave = 150; // N: a hard pull back on each handle

const m40 = { section: profile };
const cart = Assembly.named("Cart")
  .point("rear", [0, 0, z]).point("rearL", [0, rearTrack / 2, z]).point("rearR", [0, -rearTrack / 2, z])
  .point("front", [wheelbase, 0, z]).point("frontL", [wheelbase, frontTrack / 2, z]).point("frontR", [wheelbase, -frontTrack / 2, z])
  .point("seatFoot", [seatAt, 0, z]).point("seat", [seatAt, 0, z + seatHeight])
  .point("seatL", [seatAt, seatWidth / 2, z + seatHeight]).point("seatR", [seatAt, -seatWidth / 2, z + seatHeight])
  .point("backTop", [0, 0, z + backrestHeight])
  .point("columnFoot", [columnFoot, 0, z]).point("columnTop", [columnTop[0], 0, z + columnTop[1]])
  .point("handleL", [columnTop[0], handleWidth / 2, z + columnTop[1]]).point("handleR", [columnTop[0], -handleWidth / 2, z + columnTop[1]])
  // Axle beams run through; the spine joins them, and everything else stands on the spine.
  .member("rear_beam", ["rearL", "rear", "rearR"], { ...m40, priority: 10 })
  .member("front_beam", ["frontL", "front", "frontR"], { ...m40, priority: 10 })
  .member("spine", ["rear", "seatFoot", "columnFoot", "front"], { ...m40, priority: 5 })
  .member("seat_post", ["seatFoot", "seat"], m40)
  .member("seat_rail", ["seatL", "seat", "seatR"], { ...m40, priority: 4 })
  .member("backrest", ["rear", "backTop"], m40)
  .member("steering_column", ["columnFoot", "columnTop"], m40)
  .member("handle", ["handleL", "columnTop", "handleR"], { ...m40, priority: 4 })
  .joints(joint)
  // On its four wheels: free to roll, held sideways on the left.
  .support("rearL", "pinned")
  .support("rearR", { x: true, z: true })
  .support("frontL", { y: true, z: true })
  .support("frontR", "roller")
  // Sitting: most of the weight on the seat, feet on the front beam.
  .force("Child seated", "seatL", [0, 0, -0.4 * child])
  .force("Child seated", "seatR", [0, 0, -0.4 * child])
  .force("Child seated", "frontL", [0, 0, -0.1 * child])
  .force("Child seated", "frontR", [0, 0, -0.1 * child])
  // Dropping off a kerb: the same, at 2.5 g.
  .force("Off a kerb, 2.5 g", "seatL", [0, 0, -child])
  .force("Off a kerb, 2.5 g", "seatR", [0, 0, -child])
  .force("Off a kerb, 2.5 g", "frontL", [0, 0, -0.25 * child])
  .force("Off a kerb, 2.5 g", "frontR", [0, 0, -0.25 * child])
  // Leaning back and heaving on the handles, back against the backrest.
  .force("Heaving on the handles", "handleL", [-heave, 0, 0])
  .force("Heaving on the handles", "handleR", [-heave, 0, 0])
  .force("Heaving on the handles", "backTop", [-2 * heave, 0, 0])
  .force("Heaving on the handles", "seatL", [0, 0, -0.5 * child])
  .force("Heaving on the handles", "seatR", [0, 0, -0.5 * child]);

export default Model.named("Kids' cart")
  .info({
    designer: "topo-cad",
    date: "2026-10-05",
    design_basis: [
      "item Profile 8 40 x 40 light (AlMgSi0.5 F25), with the maker's section properties",
      "Rider 37 kg. Joints analysed as rigid and checked against the connector maker's ratings",
      "Elastic stress against yield with a factor of safety of 2; displacement limited to 5 mm",
    ],
    notes: ["Wheels, axles, steering and seat panels are not modelled: the four wheel positions are the supports."],
  })
  .designBasis({ safetyFactor: 2, deflectionLimit: mm(5) })
  .add(cart)
  .sheets(
    sheet("K-101", "General arrangement", elevation(cart, { from: "south", title: "Side elevation" }), plan(cart, { title: "Plan" }), iso(cart, { title: "Kids' cart" })),
    sheet(
      "K-201",
      "Deflection",
      deflection(cart, { from: "south", combination: "Self-weight + Off a kerb, 2.5 g", title: "Off a kerb" }),
      deflection(cart, { from: "south", combination: "Self-weight + Heaving on the handles", title: "Heaving on the handles" }),
    ),
    sheet("K-301", "Checks", utilization(cart, { from: "south" }), schedule("checks"), schedule("reactions")),
    sheet("K-401", "Schedules", schedule("members"), schedule("connections")),
  );
