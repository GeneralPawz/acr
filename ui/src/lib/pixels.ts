/** Window pixels of the game per CSS pixel. The browser build sizes its canvas in CSS pixels
 * (its backing store has `clientWidth` pixels whatever `devicePixelRatio` says), and the game's
 * window pixels are the canvas'; a native webview has no canvas and uses the device ratio. */
export function pixelRatio(): number {
  const canvas = document.querySelector('canvas');
  if (canvas && canvas.clientWidth > 0 && canvas.width > 0) return canvas.width / canvas.clientWidth;
  return window.devicePixelRatio || 1;
}
