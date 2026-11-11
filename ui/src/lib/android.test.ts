import { afterEach, describe, expect, it, vi } from 'vitest';
import { androidBridge, onAndroid, regionsOf, watchRegions } from './android';

const rect = (x: number, y: number, width: number, height: number) =>
  ({ x, y, width, height, left: x, top: y, right: x + width, bottom: y + height, toJSON: () => ({}) }) as DOMRect;

afterEach(() => {
  document.body.innerHTML = '';
  delete (window as unknown as { acr?: unknown }).acr;
  vi.restoreAllMocks();
});

function stage(x: number, y: number, w: number, h: number, hit = true) {
  const el = document.createElement('div');
  if (hit) el.setAttribute('data-acr-hit', '');
  el.getBoundingClientRect = () => rect(x, y, w, h);
  document.body.append(el);
  return el;
}

describe('Android host', () => {
  it('knows it runs there by the Java bridge', () => {
    expect(onAndroid()).toBe(false);
    (window as unknown as { acr: unknown }).acr = { post: () => {}, regions: () => {} };
    expect(onAndroid()).toBe(true);
  });

  it('publishes the rectangles of the screens the UI draws, rounded, and nothing else', () => {
    stage(100.04, 0, 800, 600.26);
    stage(0, 0, 5, 5, false);
    expect(regionsOf()).toBe('100,0,800,600.3');
    document.body.innerHTML = '';
    expect(regionsOf()).toBe('');
  });

  it('reports a change once per frame and not an unchanged state', async () => {
    const frames: FrameRequestCallback[] = [];
    vi.spyOn(window, 'requestAnimationFrame').mockImplementation((cb) => frames.push(cb));
    const reported: string[] = [];
    const stop = watchRegions((csv) => reported.push(csv));
    expect(reported).toEqual(['']);
    const el = stage(1, 2, 3, 4);
    await Promise.resolve();
    el.setAttribute('style', 'left: 1px');
    el.setAttribute('style', 'left: 2px');
    await Promise.resolve();
    expect(frames).toHaveLength(1);
    frames.shift()!(0);
    expect(reported).toEqual(['', '1,2,3,4']);
    window.dispatchEvent(new Event('resize'));
    frames.shift()!(0);
    expect(reported).toEqual(['', '1,2,3,4']);
    stop();
  });

  it('sends over acr.post and fetches assets from the origin', async () => {
    const posted: string[] = [];
    (window as unknown as { acr: unknown }).acr = { post: (j: string) => posted.push(j), regions: () => {} };
    const bridge = androidBridge();
    bridge.send({ t: 'resync' });
    expect(posted).toEqual(['{"t":"resync"}']);
    await expect(bridge.assetUrl('bmp/a.png')).resolves.toBe('https://acr.local/asset/v1/bmp/a.png');
  });
});
