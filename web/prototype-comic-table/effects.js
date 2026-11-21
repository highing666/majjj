// PROTOTYPE: 2D comic impact frame bursts (speedlines, skewed cut-in slash, lacquer splash).

function buildSpeedlinesSvg(strokeColor) {
  const rays = [];
  const count = 28;
  for (let i = 0; i < count; i++) {
    const angle = (i * 360) / count;
    const len = i % 2 === 0 ? 44 : 32;
    const width = i % 3 === 0 ? 4.2 : 2.1;
    rays.push(
      `<line x1="200" y1="200" x2="${(200 + Math.cos((angle * Math.PI) / 180) * (120 + len)).toFixed(1)}" y2="${(200 + Math.sin((angle * Math.PI) / 180) * (120 + len)).toFixed(1)}" stroke="${strokeColor}" stroke-width="${width}" stroke-dasharray="48 160" stroke-dashoffset="-48" stroke-linecap="round"/>`
    );
  }
  return `<svg class="impact-speedlines" viewBox="0 0 400 400" xmlns="http://www.w3.org/2000/svg" aria-hidden="true">${rays.join('')}</svg>`;
}

export function triggerComicImpact(container, { variant = 'A', kind = 'pon', label = '碰!', sublabel = '', seat = 0 }) {
  if (!container) return;

  const existing = container.querySelector('.comic-impact-layer');
  if (existing) existing.remove();

  const layer = document.createElement('div');
  layer.className = `comic-impact-layer impact-variant-${variant} impact-kind-${kind} impact-seat-${seat}`;

  if (variant === 'A') {
    const color = kind === 'hu' ? '#F3A93B' : kind === 'kong' ? '#58D1C9' : '#E25343';
    layer.innerHTML = `
      <div class="impact-flash impact-flash--warm"></div>
      <div class="impact-burst-center">
        ${buildSpeedlinesSvg(color)}
        <div class="impact-stamp impact-stamp--piltover">
          <span class="impact-stamp-main">${label}</span>
          ${sublabel ? `<span class="impact-stamp-sub">${sublabel}</span>` : ''}
        </div>
      </div>
    `;
  } else if (variant === 'B') {
    layer.innerHTML = `
      <div class="impact-flash impact-flash--zaun"></div>
      <div class="impact-cutin-strip">
        <div class="impact-cutin-halftone"></div>
        <div class="impact-cutin-text">
          <span class="impact-cutin-label">${label}</span>
          ${sublabel ? `<span class="impact-cutin-sub">${sublabel}</span>` : ''}
        </div>
      </div>
    `;
  } else {
    layer.innerHTML = `
      <div class="impact-flash impact-flash--parlor"></div>
      <div class="impact-seal-burst">
        <div class="impact-ink-ring"></div>
        <div class="impact-seal-stamp">
          <span class="impact-seal-main">${label}</span>
          ${sublabel ? `<span class="impact-seal-sub">${sublabel}</span>` : ''}
        </div>
      </div>
    `;
  }

  container.appendChild(layer);

  // Short damped table shake on high-impact events
  if (kind === 'hu' || kind === 'kong' || kind === 'pon') {
    container.classList.remove('is-shaking');
    void container.offsetWidth;
    container.classList.add('is-shaking');
    setTimeout(() => container.classList.remove('is-shaking'), 240);
  }

  const duration = kind === 'hu' ? 1150 : 860;
  setTimeout(() => {
    layer.classList.add('is-fading');
    setTimeout(() => layer.remove(), 220);
  }, duration);
}
