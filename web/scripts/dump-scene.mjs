// Prints the complete scene (JSON) of a .scad file, evaluated with the browser engine in Node.
//
//   node web/scripts/dump-scene.mjs <file.scad> [--module name] [--meshes] [--summary]
//
// Used to inspect scenes and to write Vitest fixtures.
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const pkg = join(here, "../src/wasm-pkg");
const args = process.argv.slice(2);
const mi = args.indexOf("--module");
const module = mi >= 0 ? args[mi + 1] : undefined;
const file = args.find((a, i) => !a.startsWith("--") && !(mi >= 0 && i === mi + 1));
const withMeshes = args.includes("--meshes");
const summary = args.includes("--summary");

const { default: createSmlib } = await import(pathToFileURL(join(pkg, "smlib.js")).href);
const { initSmlib } = await import(pathToFileURL(join(pkg, "smlib-bridge.js")).href);
await initSmlib(createSmlib, { wasmBinary: readFileSync(join(pkg, "smlib.wasm")) });
const cad = await import(pathToFileURL(join(pkg, "cad_wasm.js")).href);
await cad.default({ module_or_path: readFileSync(join(pkg, "cad_wasm_bg.wasm")) });
const s = new cad.Session();

const out = [];
const run = () => {
  while (s.busy()) out.push(...s.step(1000));
};
if (module) s.post({ type: "SetContext", module, overrides: {} });
out.push(...s.post({ type: "SetSource", text: readFileSync(file, "utf8"), version: 1 }));
run();
const scene = out.filter((m) => m.type === "Scene").at(-1);
const diags = out.filter((m) => m.type === "Diagnostics").at(-1);
let meshes = [];
if (withMeshes) {
  out.length = 0;
  s.post({ type: "RequestMeshes", nodes: scene.nodes.filter((n) => n.mesh_key).map((n) => n.idx), version: 1 });
  run();
  meshes = out
    .filter((m) => m.type === "Meshes")
    .flatMap((m) => m.items)
    .map((m) => {
      const o = { ...m };
      for (const k of ["positions", "normals", "indices", "tri_tags", "tri_faces", "edge_segments", "edge_ids"]) o[k] = [...m[k]];
      return o;
    });
}
if (summary) {
  const p = (...a) => console.log(...a);
  for (const n of scene.nodes)
    p(n.idx, "par", n.parent, n.label, "stmt", n.stmt, "L" + n.range.line, n.status, "mi", n.module_inst, n.subtracted ? "SUB" : "", n.dim, "tag", n.tag, JSON.stringify(n.flags), n.hints, JSON.stringify(n.bounds), "dw", n.display_world.slice(12, 15), "w", n.world.slice(12, 15));
  p("root", scene.root, "subtree", scene.root_is_subtree, "ctx", JSON.stringify(scene.context));
  p("tag_nodes", JSON.stringify(scene.tag_nodes));
  p("refs", JSON.stringify(scene.refs));
  p("hints", JSON.stringify(scene.hints.map((h) => [h.range.line, h.text, h.in_context])));
  p("instances", JSON.stringify(scene.instances.map((i) => [i.id, i.module, i.label, i.node, i.call_range?.line, i.def_range?.line, i.parent])));
  p("grids", JSON.stringify(scene.grids.map((g) => [g.id, g.name, g.in_context, g.module_inst, g.planes.map((q) => [q.axis, q.name, q.value, !!q.literal])])));
  p("anchors", JSON.stringify(scene.anchors.map((a) => [a.inst, a.name, a.text, a.point, a.dir])));
  p("modules", JSON.stringify(scene.modules.map((m) => [m.name, m.range.line, m.range.end_line, m.path])));
  p("echoes", JSON.stringify(scene.echoes));
  p("diags", JSON.stringify(diags?.items));
  p("meshes", meshes.map((m) => [m.node, m.nodes, m.indices.length / 3, m.edge_ids.length]));
} else console.log(JSON.stringify({ scene, diagnostics: diags?.items ?? [], meshes }));
