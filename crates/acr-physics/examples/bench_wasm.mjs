// Times examples/bench_wasm.rs under node: node bench_wasm.mjs path/to/bench_wasm.wasm
import { readFileSync } from "node:fs";
const { instance } = await WebAssembly.instantiate(readFileSync(process.argv[2]), {});
const { setup, run } = instance.exports;
for (const [n, half] of [[1, 0], [16, 0], [16, 1], [64, 1], [256, 1]]) {
  let best = Infinity, active = 0;
  for (let k = 0; k < 3; k++) {
    setup(n, half);
    const t = performance.now();
    active = run(240);
    best = Math.min(best, ((performance.now() - t) * 1000) / (240 * active));
  }
  console.log(`${String(n).padStart(4)} ragdolls, ${String(active).padStart(3)} active${half ? " (half frozen)" : ""}: ${best.toFixed(1)} us per active ragdoll step (wasm32, node)`);
}
