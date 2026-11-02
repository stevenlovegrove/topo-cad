// The sheet stack (examples/sheet-stack.ts), written on the part core: a board on four standoffs on a plate.
//
// The board is 40 by 30, 2 thick, of FR-4, with four plain holes of 3.2 through it, 30 by 20 apart. The standoffs are
// 10 long, 5 square, tapped M3 4 deep in each end. `stack` (topo-part/fixings) makes of those what would be
// written by hand: a hole bored in the plate under each of the board's holes, of the size an M3 goes through freely;
// the four standoffs, each where its hole is; and a screw at every joint, four up through the plate into the
// standoffs' feet and four down through the board into their tops, each as long as it should be.
//
// Where the board is on the plate is said two ways. Along the plate it is centred: its holes are as far in from the
// plate's two ends, so it stays in the middle if the plate is made longer. Across it, nothing is said, and it stays
// where it is drawn. How high it is is where its standoffs' feet are on the plate.

import { part, free, plate, must, is, placed, joints, sub, div, materials, mm } from "topo-part";
import { stack } from "topo-part/fixings";

const [length, width, thick] = [mm(100), mm(60), mm(3)];
const [boardLength, boardThick] = [mm(40), mm(2)];

export default part("sheet stack", () => {
  const base = plate("plate", length, width, thick, materials.aluminium6061T6);
  // (It moves, and does not turn: its corner where it is drawn, 25 along, 15 across, a little above where it comes.)
  const moved = { x: free(mm(25)), y: free(mm(15)), z: free(mm(20)) };
  const hole = (x: number, y: number) => ({ at: mm.each([x, y] as [number, number]), r: mm(1.6) });
  const board = plate("board", boardLength, mm(30), boardThick, materials.FR4, [hole(5, 5), hole(35, 5), hole(35, 25), hole(5, 25)], { at: placed({ joint: joints.moves(moved) }) });
  const centres = { m1: board.centres[0], m2: board.centres[1], m3: board.centres[2], m4: board.centres[3] };
  return {
    base,
    mounted: stack("mounted", { holding: { moved }, plate: base, top: base.faces["+z"], plateThickness: thick, board, centres, boardThickness: boardThick }),
    says: { centred: must(is(moved.x, div(sub(length, boardLength), 2))) },
  };
});
