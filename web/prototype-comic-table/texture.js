// PROTOTYPE: Procedural seamless painterly impasto & paper grain textures generated once at startup.

const textureCache = new Map();

function mulberry32(seed) {
  let t = seed >>> 0;
  return () => {
    t += 0x6d2b79f5;
    let r = Math.imul(t ^ (t >>> 15), 1 | t);
    r ^= r + Math.imul(r ^ (r >>> 7), 61 | r);
    return ((r ^ (r >>> 14)) >>> 0) / 4294967296;
  };
}

function drawToroidalStroke(ctx, size, x, y, w, h, angle, color, alpha) {
  ctx.save();
  ctx.globalAlpha = alpha;
  ctx.fillStyle = color;
  for (let dx = -size; dx <= size; dx += size) {
    for (let dy = -size; dy <= size; dy += size) {
      ctx.save();
      ctx.translate(x + dx, y + dy);
      ctx.rotate(angle);
      ctx.beginPath();
      ctx.roundRect(-w * 0.5, -h * 0.5, w, h, Math.min(w, h) * 0.35);
      ctx.fill();
      ctx.restore();
    }
  }
  ctx.restore();
}

export function getImpastoTextureUrl(style = 'piltover') {
  if (textureCache.has(style)) {
    return textureCache.get(style);
  }
  if (typeof document === 'undefined') return '';

  const size = 256;
  const canvas = document.createElement('canvas');
  canvas.width = size;
  canvas.height = size;
  const ctx = canvas.getContext('2d');
  if (!ctx) return '';

  const palettes = {
    piltover: {
      seed: 104729,
      baseAngle: -0.42,
      light: 'rgba(246, 214, 152, 1)',
      mid: 'rgba(54, 114, 118, 1)',
      dark: 'rgba(10, 22, 28, 1)',
    },
    zaun: {
      seed: 221933,
      baseAngle: 0.35,
      light: 'rgba(72, 226, 210, 1)',
      mid: 'rgba(42, 54, 84, 1)',
      dark: 'rgba(7, 9, 16, 1)',
    },
    parlor: {
      seed: 398217,
      baseAngle: -0.25,
      light: 'rgba(245, 206, 128, 1)',
      mid: 'rgba(55, 108, 88, 1)',
      dark: 'rgba(14, 34, 27, 1)',
    },
  };

  const p = palettes[style] || palettes.piltover;
  const rand = mulberry32(p.seed);

  ctx.clearRect(0, 0, size, size);

  // Broad dry-brush painterly strokes
  for (let i = 0; i < 160; i++) {
    const x = rand() * size;
    const y = rand() * size;
    const w = 18 + rand() * 44;
    const h = 3 + rand() * 8;
    const angle = p.baseAngle + (rand() - 0.5) * 0.45;
    const roll = rand();
    const color = roll < 0.34 ? p.light : roll < 0.72 ? p.mid : p.dark;
    const alpha = roll < 0.34 ? 0.03 + rand() * 0.045 : 0.05 + rand() * 0.07;
    drawToroidalStroke(ctx, size, x, y, w, h, angle, color, alpha);
  }

  // Fine canvas tooth / paper grain
  for (let i = 0; i < 650; i++) {
    const x = rand() * size;
    const y = rand() * size;
    const w = 1.5 + rand() * 3;
    const h = 1 + rand() * 2;
    const isLight = rand() > 0.52;
    ctx.fillStyle = isLight ? 'rgba(255,248,232,0.035)' : 'rgba(6,10,14,0.055)';
    ctx.fillRect(x, y, w, h);
  }

  const url = canvas.toDataURL('image/png');
  textureCache.set(style, url);
  return url;
}
