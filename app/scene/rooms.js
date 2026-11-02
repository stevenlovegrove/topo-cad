// Rooms: named boxes in the model's frame, kept with the project (people and agents name them);
// drawn in the views, and counted in the tree (kinds.js).

import { vis } from "../objects.js";
import { st, toScan } from "./state.js";
import { corners } from "./geom.js";
import { kind } from "./kinds.js";

kind("room", {
  group: "rooms", order: 70,
  rows: (row) => (st.rooms.length ? row(0, "top", "rooms", "Rooms", { note: st.rooms.length }) : ""),
  marks(out) {
    if (vis("rooms")) for (const r of st.rooms) out.box(corners(r).map(toScan), [120, 170, 255]);
  },
});
