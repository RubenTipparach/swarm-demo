// Export every stock hull from a redux-tribes checkout as a sparse voxel file.
//
//     node tools/export_hulls.mjs <path to redux-tribes> assets/hulls
//
// Bundles redux-tribes' `design.ts` in memory, the way its own tests do, and
// asks its rasteriser for each stock hull. Nothing here interprets a design:
// the cells, materials, purposes, livery roles and resolved colours are all
// redux-tribes' own answers, written down. Needs `esbuild` and `three` on the
// NODE_PATH or beside this script; the redux-tribes checkout is read and never
// written.
//
// FTVX v1, little endian:
//   "FTVX", u32 version = 1, u32 nx, u32 ny, u32 nz, f32 cell, u32 count,
//   then count records of: u32 index, u8 mat, u8 purp, u8 tone, u8 pad, u32 rgb
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
const entry = resolve(rt, 'web/src/app/design.ts');
mkdirSync(outDir, { recursive: true });
const r = await build({ entryPoints: [entry], bundle: true, write: false, format: 'esm',
  platform: 'node', absWorkingDir: resolve(rt, 'web'), logLevel: 'silent' });
const code = r.outputFiles[0].text;
const mod = await import('data:text/javascript;base64,' + Buffer.from(code).toString('base64'));
const { NX, NY, NZ, RUNG, Mat, stockFor, rasterise, frameFor, armourColour, cellColour } = mod;
const KEYS = ['terran_frigate','karisen_frigate','rogue_frigate','benefactor_frigate','freighter',
  'terran_corvette','terran_destroyer','terran_cruiser','karisen_corvette','karisen_destroyer','karisen_cruiser',
  'rogue_corvette','rogue_destroyer','rogue_cruiser','benefactor_corvette','benefactor_destroyer','benefactor_cruiser',
  'civil_lighter','civil_hauler','civil_boxship','civil_tanker','civil_miner','civil_liner'];
let total = 0;
const manifest = [];
for (const key of KEYS) {
  const d = stockFor(key);
  const frame = frameFor(key);
  const cell = RUNG[frame.rung];
  const ras = rasterise(d);
  const recs = [];
  for (let n = 0; n < NX * NY * NZ; n++) {
    const mat = ras.grid[n];
    if (!mat) continue;
    const rgb = (mat === Mat.Plate || mat === Mat.Skinned)
      ? armourColour(d.faction, d.paint, ras.tone[n])
      : cellColour(mat, ras.purp[n], d.paint);
    recs.push([n, mat, ras.purp[n], ras.tone[n], rgb >>> 0]);
  }
  const buf = Buffer.alloc(28 + recs.length * 12);
  let o = 0;
  buf.write('FTVX', o); o += 4;
  buf.writeUInt32LE(1, o); o += 4;
  buf.writeUInt32LE(NX, o); o += 4; buf.writeUInt32LE(NY, o); o += 4; buf.writeUInt32LE(NZ, o); o += 4;
  buf.writeFloatLE(cell, o); o += 4;
  buf.writeUInt32LE(recs.length, o); o += 4;
  for (const [n, mat, purp, tone, rgb] of recs) {
    buf.writeUInt32LE(n, o); o += 4;
    buf.writeUInt8(mat, o++); buf.writeUInt8(purp, o++); buf.writeUInt8(tone, o++); buf.writeUInt8(0, o++);
    buf.writeUInt32LE(rgb, o); o += 4;
  }
  writeFileSync(`${outDir}/${key}.ftvx`, buf);
  total += buf.length;
  manifest.push({ key, faction: d.faction, rung: frame.rung, cell, cells: recs.length, plate: ras.plateCells, bytes: buf.length });
  console.log(`${key.padEnd(22)} ${frame.rung.padEnd(8)} cell ${cell.toFixed(6)}  cells ${String(recs.length).padStart(6)}  plate ${String(ras.plateCells).padStart(6)}  ${buf.length} B`);
}
writeFileSync(`${outDir}/manifest.json`, JSON.stringify(manifest, null, 1));
console.log(`total ${total} bytes over ${KEYS.length} hulls`);
