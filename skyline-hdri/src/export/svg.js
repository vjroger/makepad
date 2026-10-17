// Vector export: one closed polygon per distance band in plate pixel coordinates.

/** Douglas-Peucker on an array of [x, y] points; tolerance in pixels. */
export function simplify(points, tolerance = 0.3) {
  if (points.length < 3) return points;
  const keep = new Uint8Array(points.length);
  keep[0] = keep[points.length - 1] = 1;
  const stack = [[0, points.length - 1]];
  while (stack.length) {
    const [a, b] = stack.pop();
    const [ax, ay] = points[a], [bx, by] = points[b];
    const dx = bx - ax, dy = by - ay, len = Math.hypot(dx, dy) || 1;
    let maxD = 0, idx = -1;
    for (let i = a + 1; i < b; i++) {
      const [px, py] = points[i];
      const d = Math.abs(dy * px - dx * py + bx * ay - by * ax) / len;
      if (d > maxD) { maxD = d; idx = i; }
    }
    if (maxD > tolerance && idx > 0) { keep[idx] = 1; stack.push([a, idx], [idx, b]); }
  }
  return points.filter((_, i) => keep[i]);
}

/** Row (fractional, top-down) of an elevation angle in radians. */
export function rowOfAngle(e, height) { return (0.5 - e / Math.PI) * height; }

/**
 * Build the SVG. bands: [{ label, radius, angles: Float32Array(W) }], ordered far -> near.
 * Each polygon is closed along the bottom edge so it can be filled or carved as a layer.
 */
export function plateToSVG(plate, params, opts = {}) {
  const { width: W, height: H } = plate;
  const tol = opts.tolerance ?? 0.3;
  const layers = [];
  const bandRadii = params.bands || [];
  // far to near: full horizon first, then each band from the largest radius down
  layers.push({ label: `all (to ${(params.maxDistance / 1000).toFixed(0)} km)`, angles: plate.horizon });
  for (let b = bandRadii.length - 1; b >= 0; b--) {
    layers.push({ label: `within ${(bandRadii[b] / 1000).toFixed(bandRadii[b] >= 1000 ? 0 : 1)} km`, angles: plate.bands.subarray(b * W, (b + 1) * W) });
  }
  const n = layers.length;
  const shade = (i) => Math.round(225 - (i / Math.max(1, n - 1)) * 150);
  let body = '';
  layers.forEach((layer, i) => {
    if (!layer.angles) return;
    const pts = [];
    for (let x = 0; x < W; x++) pts.push([x + 0.5, rowOfAngle(layer.angles[x], H)]);
    const simp = simplify(pts, tol);
    const d = `M0,${H} L0,${simp[0][1].toFixed(2)} ` + simp.map(([x, y]) => `L${x.toFixed(2)},${y.toFixed(2)}`).join(' ') + ` L${W},${simp[simp.length - 1][1].toFixed(2)} L${W},${H} Z`;
    const g = shade(i);
    body += `  <path id="band-${i}" data-label="${layer.label}" fill="rgb(${g},${g},${g})" d="${d}"/>\n`;
  });
  const meta = `<!-- skyline-hdri: lat ${params.lat} lon ${params.lon} eyeZ ${params.eyeZ?.toFixed?.(1) ?? ''} heading ${params.headingOffset} deg; x = azimuth (-180..180 deg, centre = north), y = elevation (+90 top .. -90 bottom) -->`;
  return `<?xml version="1.0" encoding="UTF-8"?>\n<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${W} ${H}" width="${W}" height="${H}">\n${meta}\n${body}</svg>\n`;
}
