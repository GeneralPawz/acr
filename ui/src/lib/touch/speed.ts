/** The game speed the touch skin offers: a few stops of about equal perceived steps instead of the options'
 * 0..=100 slider, whose speed is hyperbolic in `FPSTime` (docs/simulation.md "Game speed"), so a linear
 * slider is mostly "normal" and then "very fast" in its last few pixels. */

/** `FPSTime` of each stop, slowest first; 50 is the original's default speed. */
export const STOPS = [100, 70, 50, 35, 25, 17, 10, 5] as const;
/** The stop of the default speed. */
export const NORMAL = STOPS.indexOf(50);

/** Simulation ticks per second at `FPSTime` (`1000 / (2 * FPSTime)`, 5 at the slowest, 100 at most: the
 * game's `tick_rate`). */
export function ticksPerSecond(fpsTime: number): number {
  return Math.floor(1000 / (2 * Math.min(Math.max(fpsTime, 5), 100)));
}

/** The stop nearest to the options slider's position `100 - FPSTime` (as `TouchHud.speed` carries it),
 * measured in ticks per second on a log scale, which is how the eye judges speed. */
export function nearestStop(speed: number): number {
  const rate = Math.log(ticksPerSecond(100 - speed));
  let best = 0;
  for (let i = 1; i < STOPS.length; i++) {
    if (Math.abs(Math.log(ticksPerSecond(STOPS[i])) - rate) < Math.abs(Math.log(ticksPerSecond(STOPS[best])) - rate)) best = i;
  }
  return best;
}

/** The value `bar/speed` takes for a stop: the options slider's position, `100 - FPSTime`. */
export function speedOf(stop: number): number {
  return 100 - STOPS[Math.min(Math.max(Math.round(stop), 0), STOPS.length - 1)];
}
