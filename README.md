# topo-cad

Topology-first CAD-as-code for things made of members — house framing, and
also a bicycle frame or a cart of aluminium extrusion. You describe *what connects
to what* (nodes, members, junctions); solids, cut lengths, engineering drawings,
schedules and the analytical model are derived. See [DESIGN.md](DESIGN.md).

```bash
cargo test --workspace
cargo run --release -p topo-cli -- run examples/garage-as-built.ts out   # TypeScript model
web/build.sh                                                             # the web app's engine (wasm) → web/pkg
cargo run --release -p topo-cli -- serve examples                         # web app: http://127.0.0.1:8765
cargo run --release -p topo-cli -- site site-out examples                 # the app + examples for any static web server
cargo run --release -p topo-cli -- example garage out                    # built-in Rust example
cargo run --release -p topo-cli -- render out/model.json out2            # JSON IR
tsc -p examples                                                          # type-check models
```

### The web app

The app is static files ([web/](web)): the page, a Web Worker running the
engine compiled to wasm, and the model scripts run by the browser's own
JavaScript engine. It needs no server of its own. Projects live in one of:

- **Site files**: `.ts` files beside the app on any web server, listed in
  `files/manifest.json` (`topo site` writes both). Read-only there; your
  edits are kept in your browser.
- **A folder on this computer** (Chrome, Edge): your real files, read and
  written in place; edits made elsewhere (an editor, an LLM helper) show up
  within a second.
- **This browser**: projects kept in the browser's storage.

`topo serve` is a small local server for the same app that also lets it save
to the folders you give it (`PUT /files/<id>`), and notices outside edits.

Models are TypeScript modules importing `topo-cad` ([ts/topo-cad.ts](ts/topo-cad.ts));
see [examples/garage-as-built.ts](examples/garage-as-built.ts),
[examples/custom-trusses.ts](examples/custom-trusses.ts) (custom truss shapes,
repeated assemblies) and [examples/truss-gallery.ts](examples/truss-gallery.ts). Every builder
method returns a new value, so templates are shared and specialised freely.

Frames are explicit: the world is +x east, +y north, +z up; a `Building` is
placed in it (`.placed({ origin, xBearing })`, several make a `Site`); groups
and members have their own frames. Member faces are named canonically (`±x`
ends along the grain, `±y` wide faces, `±z` edges), or picked by direction:
`facing(member("Wall A/cap_plate"), "up")`, `facing(stud, "inside", { in: "Wall A" })`.
Field measurements (`horizontal`, `vertical`, `along(a, b, "north")`,
`lengthOf`, `riseOver`) fit `unknown(...)` parameters; see
[examples/garage-truss.ts](examples/garage-truss.ts).

Structural checks run on every model: a gravity load takedown (roof →
trusses → plates → studs/headers → foundation) and NDS (ASD) member and
nailed-connection checks, each with a hand-calculation-style trace citing its
source tables ([crates/topo-data](crates/topo-data/data)). `topo serve` shows
them as heatmaps and a member inspector; `utilization(...)`,
`schedule("checks")` and `schedule("reactions")` put them on sheets.
[examples/benchmarks](examples/benchmarks) holds worked problems checked
against hand calculations and published tables; start a site file from
[examples/site-template.ts](examples/site-template.ts), or use a ready-made
region such as [unincorporated King County, WA](examples/sites/king-county-wa.ts).

### Not only buildings

The core is domain-general, and two examples keep it so:

- [examples/bike.ts](examples/bike.ts): a road bike from a geometry chart
  (stack, reach, angles…) as named points and round tubes. A scenario
  switches the same topology between brazed steel, TIG-welded 6061-T6 and
  moulded carbon; load cases are riding situations (seated, sprinting on one
  pedal, braking).
- [examples/kids-cart.ts](examples/kids-cart.ts): a kids' cart of 40 x 40
  T-slot profile joined with catalog connectors (angle brackets, end
  fasteners), checked against the connector maker's ratings.

Such a model is `Model.named(...)` rather than `Building.named(...)`:
millimetres, all members analysed as one 3D frame with joints as they are
made (`Joint.welded()`, `Connector.angleBracket({ count: 2 })`), and checks
by plain mechanics (stress against yield, buckling, displacement, joint
ratings) with the same traces and sourced data
([materials](crates/topo-data/data/materials.json),
[joint zones](crates/topo-data/data/joint-zones.json),
[T-slot profiles](crates/topo-data/data/tslot-profiles.json) and
[connectors](crates/topo-data/data/tslot-connectors.json)). The 3D tab
shows the deflected shape; `deflection(...)` puts it on a sheet.

The drawing set is code too: `.sheets(standardSheets("cover"), sheet("S-102",
"Typical truss", elevation(roof.truss(2)), detail(heel, [roof.truss(2), walls],
{ scale: '1-1/2"' })), standardSheets("walls"))` — the sheets are listed in
the app's browser. The app shows views in panes side by side (a drawing, the
3D model, the script to edit in place), so a sheet can sit beside its script.

Given a lidar scan, `topo serve MODEL.ts SCAN.ply` makes the app a **scan
workspace**: people and LLM agents (over MCP) work the model against the
scan together, with issues pinned to places, proposals to the model as
diffs, and the project as a git repository. See
[docs/scan-workspace.md](docs/scan-workspace.md).

`out/` gets: sheet SVGs (`G-001`, `S-101`, `S-201`, …) + `index.html`, DXF per
sheet (paper space) and per view (full-size model space), `model.json` (the IR),
`model.obj`, `analysis-model.json`, and `report.txt` (junction census + issues).

## Crates

| crate | role |
|---|---|
| `topo-core` | model IR, builder API, topology, validation, units |
| `topo-geom` | junction resolution → exact member solids, clash detection, mesh |
| `topo-draw` | hidden-line views, dimensions, schedules, sheets, SVG/DXF |
| `topo-timber` | lumber + NDS data, fastening presets, wall/floor generators, examples |
| `topo-fab` | tube and extrusion frames: sections, T-slot profiles, metals and laminates, welds/brazes/bonds, catalog connectors |
| `topo-analysis` | analytical model, support graph, gravity load takedown, plane- and 3D-frame solvers, combinations, NDS and plain-mechanics checks |
| `topo-data` | reference tables with provenance: NDS values and factors, fasteners, material weights, engineering materials, T-slot catalogs |
| `topo-script` | TypeScript models → one JS bundle (oxc) → scene spec → model; runs natively in QuickJS |
| `topo-app` | the app engine: a model session answering the UI's requests (no server, files or clock) |
| `topo-wasm` | wasm-bindgen: the app engine for the browser, and the `Project` API |
| `topo-scan` | a model against a lidar scan: registration, per-member evidence, plane fits, sections; the scan workspace (issues, proposals, MCP tools) |
| `topo-cli` | `topo` binary: run/render models; `serve` and `site` for the web app, and the scan workspace service |

## Minimal example

```rust
use topo_core::{*, units::*};
use topo_timber::{lumber, Wall, Opening};

let mut m = Model::new("Shed");
let dfl2 = m.add_material(lumber::graded("DFL", "No.2"));
Wall::new("North", v3(0., 0., 0.), v3(ft(12.), 0., 0.), ft_in(8., 1.125), dfl2)
    .opening(Opening::window("W1", ft(6.), ft(3.), ft(3.), inch(82.5)))
    .build(&mut m);
let topo = Topology::build(&m);
let geom = topo_geom::Geometry::build(&m, &topo);   // studs come out at 92 5/8"
```
