// A road bike as topology: named points from a geometry chart, tubes between
// them, and a joining method. Not a building — millimetres, a 3D frame
// analysis, and load cases that are riding situations.
//
// Pick the frame's material in the Loads panel (Scenario): the same points
// and tubes in brazed steel, welded aluminium or moulded carbon.
import {
  Aluminum, Assembly, Carbon, deflection, elevation, iso, Joint, kgf, mm, Model, plan, scenarios, schedule, sheet, Steel, tube, rod, utilization,
  type Joints, type MaterialSpec, type Point3, type SectionSpec,
} from "topo-cad";

// ----- geometry (a 55 cm road frame; x forward, y left, z up) -----------------
// Chart values of the Ritchey Road Logic, size 55 (ritcheylogic.com).
const stack = mm(568);
const reach = mm(395);
const headAngle = 73.5;
const seatAngle = 73.5;
const headTube = mm(160);
const seatTube = mm(525);
const chainstay = mm(415);
const bbDrop = mm(70);
const forkLength = mm(371); // axle to crown
const forkRake = mm(43);
// Wheels and parts.
const wheelRadius = mm(336); // 700 x 25c
const rearHub = mm(130);
const frontHub = mm(100);
const bbShell = mm(68);
const seatpostOut = mm(160);
const stemLength = mm(100);
const barWidth = mm(400);

const rad = (deg: number) => (deg * Math.PI) / 180;
const add = (p: Point3, d: Point3, k = 1): Point3 => [p[0] + d[0] * k, p[1] + d[1] * k, p[2] + d[2] * k];
const side = (p: Point3, y: number): Point3 => [p[0], y, p[2]];

// Rear axle at the origin's x; the bottom bracket follows from the chainstay and its drop.
const rearAxle: Point3 = [0, 0, wheelRadius];
const bb: Point3 = [Math.sqrt(chainstay ** 2 - bbDrop ** 2), 0, wheelRadius - bbDrop];
const upSeat: Point3 = [-Math.cos(rad(seatAngle)), 0, Math.sin(rad(seatAngle))];
const downSteer: Point3 = [Math.cos(rad(headAngle)), 0, -Math.sin(rad(headAngle))];
const aheadOfSteer: Point3 = [Math.sin(rad(headAngle)), 0, Math.cos(rad(headAngle))];
const seatTop = add(bb, upSeat, seatTube);
const seatCluster = add(bb, upSeat, seatTube - mm(25));
const headTop: Point3 = [bb[0] + reach, 0, bb[2] + stack];
const crown = add(headTop, downSteer, headTube);
const frontAxle = add(add(crown, downSteer, Math.sqrt(forkLength ** 2 - forkRake ** 2)), aheadOfSteer, forkRake);
const stemClamp = add(headTop, downSteer, -mm(35));
const barCentre = add(stemClamp, aheadOfSteer, stemLength);

// ----- what it is made of ------------------------------------------------------
interface FrameSystem {
  readonly material: MaterialSpec;
  readonly joint: Joints;
  readonly top: SectionSpec;
  readonly down: SectionSpec;
  readonly seat: SectionSpec;
  readonly head: SectionSpec;
  readonly chainstay: SectionSpec;
  readonly seatstay: SectionSpec;
  readonly blade: SectionSpec;
  readonly shell: SectionSpec;
}
const system = scenarios<FrameSystem>(
  {
    // Columbus SL (2020 Steel Catalogue): diameters and butt-end walls. The
    // tubes are butted (0.5–0.6 mm mid-length), which is not modelled.
    "Steel, brazed into lugs": {
      material: Steel.ColumbusOmnicrom,
      joint: Joint.brazed({ lugged: true }),
      top: tube(mm(25.4), mm(0.8)),
      down: tube(mm(28.6), mm(0.8)),
      seat: tube(mm(28.6), mm(0.8)),
      head: tube(mm(31.7), mm(1.0)),
      chainstay: tube(mm(22.2), mm(0.7)),
      seatstay: tube(mm(14), mm(0.7)),
      blade: tube(mm(24), mm(0.9)),
      shell: tube(mm(38), mm(2.0)),
    },
    // Oversize tube sizes typical of welded aluminium frames (assumed, not
    // from a catalogue).
    "Aluminium 6061-T6, TIG welded": {
      material: Aluminum.T6061,
      joint: Joint.welded({ process: "tig" }),
      top: tube(mm(31.8), mm(1.5)),
      down: tube(mm(42), mm(1.6)),
      seat: tube(mm(31.8), mm(1.6)),
      head: tube(mm(44), mm(2.0)),
      chainstay: tube(mm(22.2), mm(1.8)),
      seatstay: tube(mm(19), mm(1.4)),
      blade: tube(mm(28), mm(1.8)),
      shell: tube(mm(42), mm(3.0)),
    },
    // One moulded piece: larger sections of laminate (sizes assumed), no joints as such.
    "Carbon, moulded in one piece": {
      material: Carbon.Fabric,
      joint: Joint.moulded(),
      top: tube(mm(38), mm(1.2)),
      down: tube(mm(50), mm(1.4)),
      seat: tube(mm(34), mm(1.4)),
      head: tube(mm(48), mm(2.0)),
      chainstay: tube(mm(26), mm(1.5)),
      seatstay: tube(mm(18), mm(1.2)),
      blade: tube(mm(28), mm(1.6)),
      shell: tube(mm(46), mm(3.0)),
    },
  },
  {
    describe: {
      "Steel, brazed into lugs": "Columbus SL tube set (Omnicrom steel) in lugs",
      "Aluminium 6061-T6, TIG welded": "Oversize 6061-T6 tube; weak beside each weld",
      "Carbon, moulded in one piece": "0/90 fabric laminate guide values; indicative only",
    },
  },
);
const frame = { material: system.material };
const cockpit = { material: Aluminum.T6061 };
const steel = { material: Steel.Chromoly4130 };

// ----- loads: situations, for an 80 kg rider ------------------------------------
const W = kgf(80);
const crank = mm(172.5);
const pedalOut = mm(120); // pedal centre from the centre line
const chainring = mm(105); // radius, 52 teeth
const chainline = mm(43.5);
// Standing on the right pedal, crank forward, pulling up on the right bar.
const pull = 0.25 * W;
const pedal = W + pull;
const chain = (pedal * crank) / chainring;
// Braking at 0.6 g on the front wheel.
const brake = 0.6 * W;

const bike = Assembly.named("Bike")
  // Frame.
  .point("bb", bb).point("bbL", side(bb, bbShell / 2)).point("bbR", side(bb, -bbShell / 2))
  .point("seatCluster", seatCluster).point("seatTop", seatTop)
  .point("headTop", headTop).point("topJoint", add(headTop, downSteer, mm(30))).point("downJoint", add(crown, downSteer, -mm(35))).point("crown", crown)
  .point("rearL", side(rearAxle, rearHub / 2)).point("rearR", side(rearAxle, -rearHub / 2))
  // Fork.
  .point("crownL", side(crown, frontHub / 2)).point("crownR", side(crown, -frontHub / 2))
  .point("frontL", side(frontAxle, frontHub / 2)).point("frontR", side(frontAxle, -frontHub / 2))
  // Seatpost, stem and handlebar.
  .point("saddle", add(seatTop, upSeat, seatpostOut))
  .point("stemClamp", stemClamp).point("barCentre", barCentre)
  .point("barL", side(barCentre, barWidth / 2)).point("barR", side(barCentre, -barWidth / 2))
  .member("bb_shell", ["bbL", "bb", "bbR"], { section: system.shell, ...frame, priority: 10 })
  .member("head_tube", ["crown", "downJoint", "topJoint", "headTop"], { section: system.head, ...frame, priority: 9 })
  .member("seat_tube", ["bb", "seatCluster", "seatTop"], { section: system.seat, ...frame, priority: 8 })
  .member("down_tube", ["bb", "downJoint"], { section: system.down, ...frame, priority: 7 })
  .member("top_tube", ["seatCluster", "topJoint"], { section: system.top, ...frame, priority: 6 })
  .member("chainstay", ["bb", "rearL"], { section: system.chainstay, ...frame, priority: 5 })
  .member("chainstay", ["bb", "rearR"], { section: system.chainstay, ...frame, priority: 5 })
  .member("seatstay", ["seatCluster", "rearL"], { section: system.seatstay, ...frame, priority: 3 })
  .member("seatstay", ["seatCluster", "rearR"], { section: system.seatstay, ...frame, priority: 3 })
  .member("fork_crown", ["crownL", "crown", "crownR"], { section: tube(mm(28.6), mm(2.0)), ...frame, priority: 10 })
  .member("fork_blade", ["crownL", "frontL"], { section: system.blade, ...frame, priority: 4 })
  .member("fork_blade", ["crownR", "frontR"], { section: system.blade, ...frame, priority: 4 })
  .member("rear_axle", ["rearL", "rearR"], { section: rod(mm(10)), ...steel })
  .member("front_axle", ["frontL", "frontR"], { section: rod(mm(9)), ...steel })
  .member("seatpost", ["seatTop", "saddle"], { section: tube(mm(27.2), mm(2.2)), ...cockpit })
  .member("steerer", ["headTop", "stemClamp"], { section: tube(mm(28.6), mm(1.55)), ...steel })
  .member("stem", ["stemClamp", "barCentre"], { section: tube(mm(31.8), mm(2.0)), ...cockpit })
  .member("handlebar", ["barL", "barCentre", "barR"], { section: tube(mm(23.8), mm(1.6)), ...cockpit, priority: 10 })
  // The frame's joints, and the parts that are clamped on.
  .joints(system.joint)
  .jointAt("seatTop", Joint.clamped("Seatpost clamp"))
  .jointAt("headTop", Joint.clamped("Headset"))
  .jointAt("stemClamp", Joint.clamped("Stem clamp"))
  .jointAt("barCentre", Joint.clamped("Handlebar clamp"))
  .jointAt("rearL", Joint.clamped("Quick release"), { member: "rear_axle" })
  .jointAt("rearR", Joint.clamped("Quick release"), { member: "rear_axle" })
  .jointAt("frontL", Joint.clamped("Quick release"), { member: "front_axle" })
  .jointAt("frontR", Joint.clamped("Quick release"), { member: "front_axle" })
  // Held at the four dropouts as the wheels hold it: upright, free to roll
  // at the front, free to spread sideways on one side.
  .support("rearL", "pinned")
  .support("rearR", { x: true, z: true })
  .support("frontL", { y: true, z: true })
  .support("frontR", "roller")
  // Seated: weight on the saddle, the pedals and the bars.
  .force("Seated", "saddle", [0, 0, -0.55 * W])
  .force("Seated", "bb", [0, 0, -0.3 * W])
  .force("Seated", "barL", [0, 0, -0.075 * W])
  .force("Seated", "barR", [0, 0, -0.075 * W])
  // Sprinting: all the weight and the pull on the right pedal. The crank
  // turns freely, so its drive torque reaches the frame as chain tension
  // (bottom bracket pulled back, right dropout pulled forward); the pedal's
  // offset twists the bottom bracket.
  .force("Sprint, right pedal down", "bb", [-chain, 0, -pedal])
  .moment("Sprint, right pedal down", "bb", [pedalOut * pedal, 0, -chainline * chain])
  .force("Sprint, right pedal down", "rearR", [chain, 0, 0])
  .force("Sprint, right pedal down", "barR", [0, 0, pull])
  // Braking hard on the front wheel while seated: the rider is thrown
  // forward, the road pushes back at the tyre (a force and a moment at the
  // front axle).
  .force("Front brake, 0.6 g", "saddle", [0.55 * brake, 0, -0.55 * W])
  .force("Front brake, 0.6 g", "bb", [0.3 * brake, 0, -0.3 * W])
  .force("Front brake, 0.6 g", "barL", [0.075 * brake, 0, -0.075 * W])
  .force("Front brake, 0.6 g", "barR", [0.075 * brake, 0, -0.075 * W])
  .force("Front brake, 0.6 g", "frontL", [-brake / 2, 0, 0])
  .force("Front brake, 0.6 g", "frontR", [-brake / 2, 0, 0])
  .moment("Front brake, 0.6 g", "frontL", [0, (brake / 2) * wheelRadius, 0])
  .moment("Front brake, 0.6 g", "frontR", [0, (brake / 2) * wheelRadius, 0]);

export default Model.named("Road bike")
  .info({
    designer: "topo-cad",
    date: "2026-10-05",
    design_basis: [
      "Geometry: Ritchey Road Logic, size 55 (stack 568, reach 395, 73.5° head and seat angles, 415 chainstays, 70 drop, fork 371 / 43)",
      "Rider 80 kg. Static load cases only: no fatigue, no impact",
      "Elastic stress against yield with a factor of safety of 1.5; all members as one 3D frame",
    ],
    notes: [
      "Tubes are uniform in the model: butted walls, tapered stays and oval sections are not modelled.",
      "The head tube carries the fork at the crown only (no headset bearings); the wheels are the four supports at the dropouts.",
      "Tube ends are drawn cut flat to the tube they meet: the cope (notch) of a welded or brazed joint is not drawn.",
    ],
  })
  .designBasis({ safetyFactor: 1.5 })
  .add(bike)
  .sheets(
    sheet("B-101", "General arrangement", elevation(bike, { from: "south", title: "Side elevation" }), plan(bike, { title: "Plan" }), iso(bike, { title: "Road bike" })),
    sheet(
      "B-201",
      "Deflection",
      deflection(bike, { from: "south", combination: "Self-weight + Seated", title: "Seated" }),
      deflection(bike, { from: "south", combination: "Self-weight + Front brake, 0.6 g", title: "Front brake" }),
      deflection(bike, { from: "east", combination: "Self-weight + Sprint, right pedal down", title: "Sprint, from the front" }),
    ),
    sheet("B-301", "Checks", utilization(bike, { from: "south" }), schedule("checks"), schedule("reactions")),
    sheet("B-401", "Schedules", schedule("members"), schedule("connections")),
  );
