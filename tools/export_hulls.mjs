// Export every stock hull from a redux-tribes checkout as a sparse voxel file.
//
//     node tools/export_hulls.mjs <path to redux-tribes> assets/hulls
//
// Bundles redux-tribes' `design.ts` and `hull.ts` in memory, the way its own
// tests do, and asks ITS rasteriser and ITS mesher. Nothing here interprets a
// design: the cells, materials, purposes, livery roles, resolved colours, the
// surface each cell draws in, the finish and PBR pair of each surface, and
// every window face with its direction and decal kind are all redux-tribes'
// own answers, written down. Needs `esbuild` and `three` on the NODE_PATH or
// beside this script; the redux-tribes checkout is read and never written.
//
// FTVX v2, little endian:
//   "FTVX", u32 version = 2, u32 nx, u32 ny, u32 nz, f32 cell
//   u32 nstr, then nstr strings as u16 length + utf8 bytes
//   u32 nsurf, then nsurf records of u16 finish (string index), u16 pad,
//              f32 metalness, f32 roughness, in SURF order (hull.ts)
//   u32 ncell, then ncell records of u32 index, u8 mat, u8 purp, u8 tone,
//              u8 surf, u32 rgb
//   u32 nwin,  then nwin records of u32 cell, u8 dir (hull.ts DIRS order:
//              +x -x +y -y +z -z), u8 kind (string index), u16 variants
// Sparse, because a frigate is 65536 cells of which a few thousand are
// anything. `swarm_core::voxel::VoxelModel::from_ftvx` reads it.
import { build } from 'esbuild';
import { writeFileSync, mkdirSync } from 'node:fs';
import { resolve } from 'node:path';

const [, , rt, outDir] = process.argv;
if (!rt || !outDir) {
  console.error('usage: node tools/export_hulls.mjs <redux-tribes dir> <out dir>');
  process.exit(2);
}
mkdirSync(outDir, { recursive: true });
const load = async (rel) => {
  const r = await build({ entryPoints: [resolve(rt, rel)], bundle: true, write: false, format: 'esm',
    platform: 'node', absWorkingDir: resolve(rt, 'web'), logLevel: 'silent',
    nodePaths: [resolve(process.cwd(), 'node_modules'), resolve(new URL('.', import.meta.url).pathname, 'node_modules')] });
  return import('data:text/javascript;base64,' + Buffer.from(r.outputFiles[0].text).toString('base64'));
};
const D = await load('web/src/app/design.ts');
const H = await load('web/src/app/hull.ts');
const T = await load('web/src/app/textures.ts');
const { NX, NY, NZ, RUNG, Mat, stockFor, rasterise, frameFor, armourColour, cellColour,
  ROLE_BAND, roleAt, isPainted, paintedSlot, purposeAt, finishesOf, bandFinishes, liveryFor,
  DEFAULT_METAL, DEFAULT_ROUGH } = D;
const { hullMesh, SURF_ARMOUR, SURF_FRAME, SURF_DRIVE, SURF_WEAPON, SURF_PART, SURF_SLOT, PAINT_SLOTS, SURF_COUNT, SURF_NAMES } = H;
const { WINDOW_VARIANTS } = T;
const ARMOUR_BANDS = SURF_FRAME - SURF_ARMOUR;

const KEYS = ['terran_frigate','karisen_frigate','rogue_frigate','benefactor_frigate','freighter',
  'terran_corvette','terran_destroyer','terran_cruiser','karisen_corvette','karisen_destroyer','karisen_cruiser',
  'rogue_corvette','rogue_destroyer','rogue_cruiser','benefactor_corvette','benefactor_destroyer','benefactor_cruiser',
  'civil_lighter','civil_hauler','civil_boxship','civil_tanker','civil_miner','civil_liner'];

let total = 0;
const manifest = { surfaces: SURF_NAMES, hulls: [] };
for (const key of KEYS) {
  const d = stockFor(key);
  const frame = frameFor(key);
  const cell = RUNG[frame.rung];
  const ras = rasterise(d);
  const hm = hullMesh(d);

  // Which surface each cell draws in: hull.ts's `surfaceOf`, per cell rather
  // than per quad, and checked against every quad hull.ts actually grouped.
  const surfOf = (n) => {
    const mat = ras.grid[n];
    if (mat === Mat.Plate || mat === Mat.Skinned) {
      const t = ras.tone[n];
      if (isPainted(t)) return SURF_SLOT + paintedSlot(t);
      const band = t ? ROLE_BAND[roleAt(t)] : 0;
      return SURF_ARMOUR + (band ?? 0);
    }
    if (mat === Mat.Frame) return SURF_FRAME;
    const code = ras.purp[n];
    if (!code) return SURF_PART;
    const job = purposeAt(code);
    if (job === 'propulsion' || job === 'attitude') return SURF_DRIVE;
    if (job === 'gun' || job === 'ordnance') return SURF_WEAPON;
    return SURF_PART;
  };
  let disagree = 0, checked = 0;
  for (const g of hm.geo.groups) {
    for (let q = g.start / 6; q < (g.start + g.count) / 6; q++) {
      for (let i = hm.quadAt[q]; i < hm.quadAt[q + 1]; i++) {
        checked++;
        if (surfOf(hm.quadCells[i]) !== g.materialIndex) disagree++;
      }
    }
  }

  // The materials, exactly as hullMaterials builds them.
  const f = finishesOf(d);
  const bands = bandFinishes(d);
  const livery = liveryFor(d.faction);
  const surfaces = [];
  for (let b = 0; b < ARMOUR_BANDS; b++) {
    const pbr = livery.pbr[b];
    surfaces[SURF_ARMOUR + b] = { finish: bands[b], metal: b === 0 ? d.metal ?? DEFAULT_METAL : pbr[0], rough: b === 0 ? d.rough ?? DEFAULT_ROUGH : pbr[1] };
  }
  surfaces[SURF_FRAME] = { finish: f.frame, metal: 0.45, rough: 0.70 };
  surfaces[SURF_DRIVE] = { finish: f.drive, metal: 0.55, rough: 0.62 };
  surfaces[SURF_WEAPON] = { finish: f.weapon, metal: 0.55, rough: 0.62 };
  surfaces[SURF_PART] = { finish: f.part, metal: 0.55, rough: 0.62 };
  for (let n = 0; n < PAINT_SLOTS; n++) {
    surfaces[SURF_SLOT + n] = { finish: d.slotFinish?.[n] || f.armour, metal: d.metal ?? DEFAULT_METAL, rough: d.rough ?? DEFAULT_ROUGH };
  }

  // The window faces, off hull.ts's own window meshes: the cell is listed and
  // the direction is the quad's normal.
  const strings = [];
  const str = (s) => { let i = strings.indexOf(s); if (i < 0) { i = strings.length; strings.push(s); } return i; };
  const wins = [];
  const winCounts = {};
  for (const w of hm.windows) {
    const nrm = w.geo.getAttribute('normal').array;
    for (let q = 0; q < w.cellOf.length; q++) {
      const [x, y, z] = [nrm[q * 12], nrm[q * 12 + 1], nrm[q * 12 + 2]];
      const dir = x > 0 ? 0 : x < 0 ? 1 : y > 0 ? 2 : y < 0 ? 3 : z > 0 ? 4 : 5;
      wins.push([w.cellOf[q], dir, str(w.key), WINDOW_VARIANTS[w.key] ?? 1]);
    }
    winCounts[w.key] = w.cellOf.length;
  }
  for (const s of surfaces) str(s.finish);

  const recs = [];
  for (let n = 0; n < NX * NY * NZ; n++) {
    const mat = ras.grid[n];
    if (!mat) continue;
    const rgb = (mat === Mat.Plate || mat === Mat.Skinned)
      ? armourColour(d.faction, d.paint, ras.tone[n])
      : cellColour(mat, ras.purp[n], d.paint);
    recs.push([n, mat, ras.purp[n], ras.tone[n], surfOf(n), rgb >>> 0]);
  }

  const strBytes = strings.map(s => Buffer.from(s, 'utf8'));
  const size = 24 + 4 + strBytes.reduce((a, b) => a + 2 + b.length, 0) + 4 + surfaces.length * 12 + 4 + recs.length * 12 + 4 + wins.length * 8;
  const buf = Buffer.alloc(size);
  let o = 0;
  buf.write('FTVX', o); o += 4;
  buf.writeUInt32LE(2, o); o += 4;
  buf.writeUInt32LE(NX, o); o += 4; buf.writeUInt32LE(NY, o); o += 4; buf.writeUInt32LE(NZ, o); o += 4;
  buf.writeFloatLE(cell, o); o += 4;
  buf.writeUInt32LE(strBytes.length, o); o += 4;
  for (const b of strBytes) { buf.writeUInt16LE(b.length, o); o += 2; b.copy(buf, o); o += b.length; }
  buf.writeUInt32LE(surfaces.length, o); o += 4;
  for (const s of surfaces) {
    buf.writeUInt16LE(str(s.finish), o); o += 2; buf.writeUInt16LE(0, o); o += 2;
    buf.writeFloatLE(s.metal, o); o += 4; buf.writeFloatLE(s.rough, o); o += 4;
  }
  buf.writeUInt32LE(recs.length, o); o += 4;
  for (const [n, mat, purp, tone, surf, rgb] of recs) {
    buf.writeUInt32LE(n, o); o += 4;
    buf.writeUInt8(mat, o++); buf.writeUInt8(purp, o++); buf.writeUInt8(tone, o++); buf.writeUInt8(surf, o++);
    buf.writeUInt32LE(rgb, o); o += 4;
  }
  buf.writeUInt32LE(wins.length, o); o += 4;
  for (const [c, dir, kind, variants] of wins) {
    buf.writeUInt32LE(c, o); o += 4; buf.writeUInt8(dir, o++); buf.writeUInt8(kind, o++); buf.writeUInt16LE(variants, o); o += 2;
  }
  if (o !== size) throw new Error(`${key}: wrote ${o} of ${size}`);
  writeFileSync(`${outDir}/${key}.ftvx`, buf);
  total += buf.length;
  const groups = Object.fromEntries(hm.geo.groups.map(g => [SURF_NAMES[g.materialIndex], g.count / 6]));
  manifest.hulls.push({ key, faction: d.faction, rung: frame.rung, cell, cells: recs.length, plate: ras.plateCells,
    quads: hm.quads, groups, windows: winCounts, windowFaces: wins.length, surfaces, bytes: buf.length });
  console.log(`${key.padEnd(22)} cells ${String(recs.length).padStart(6)}  quads ${String(hm.quads).padStart(5)}  windows ${String(wins.length).padStart(4)}  surf check ${checked - disagree}/${checked}  ${buf.length} B`);
}
writeFileSync(`${outDir}/manifest.json`, JSON.stringify(manifest, null, 1));
console.log(`total ${total} bytes over ${KEYS.length} hulls`);
