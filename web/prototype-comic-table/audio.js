// PROTOTYPE: Native Web Audio API procedural SFX synthesizer & Safari AudioSession / interrupted-state guard.

class AudioEventRouter {
  constructor() {
    this.ctx = null;
    this.sfxGain = null;
    this.compressor = null;
    this.noiseBuffer = null;
    this.volume = 0.78;
    this.sessionType = 'playback';
    this.lastHoverAt = 0;
    this.log = [];
    this.listeners = new Set();
    this.stateListenerBound = false;

    if (typeof document !== 'undefined') {
      document.addEventListener('visibilitychange', () => {
        if (document.visibilityState === 'visible' && this.ctx && this.ctx.state === 'suspended') {
          // Only resume from 'suspended', never directly while 'interrupted' per W3C spec
          this.ctx.resume().catch(() => {});
        }
        this.notify();
      });
    }
  }

  subscribe(fn) {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  }

  notify() {
    const status = this.getAudioStatus();
    this.listeners.forEach((fn) => fn(status, this.log));
  }

  applyAudioSession() {
    try {
      if (typeof navigator !== 'undefined' && 'audioSession' in navigator && navigator.audioSession) {
        navigator.audioSession.type = this.sessionType;
      }
    } catch {
      // Ignore if browser rejects audioSession assignment
    }
  }

  ensureContext() {
    if (typeof window === 'undefined') return null;
    if (!this.ctx) {
      this.applyAudioSession();
      const AudioCtx = window.AudioContext || window.webkitAudioContext;
      if (!AudioCtx) return null;
      this.ctx = new AudioCtx();
      this.compressor = this.ctx.createDynamicsCompressor();
      this.compressor.threshold.value = -10;
      this.compressor.knee.value = 18;
      this.compressor.ratio.value = 6;
      this.compressor.attack.value = 0.002;
      this.compressor.release.value = 0.12;

      this.sfxGain = this.ctx.createGain();
      this.sfxGain.gain.value = this.volume;

      this.sfxGain.connect(this.compressor);
      this.compressor.connect(this.ctx.destination);
      this.noiseBuffer = this.buildNoiseBuffer(this.ctx);

      if (!this.stateListenerBound) {
        this.stateListenerBound = true;
        this.ctx.addEventListener('statechange', () => this.notify());
      }
    }
    return this.ctx;
  }

  buildNoiseBuffer(ctx) {
    const length = Math.floor(ctx.sampleRate * 0.25);
    const buffer = ctx.createBuffer(1, length, ctx.sampleRate);
    const data = buffer.getChannelData(0);
    for (let i = 0; i < length; i++) {
      data[i] = Math.random() * 2 - 1;
    }
    return buffer;
  }

  // Must be called synchronously inside pointerdown / keydown / click callback
  unlockFromGesture() {
    const ctx = this.ensureContext();
    if (!ctx) return;

    if (ctx.state === 'interrupted' || ctx.state === 'closed') {
      this.notify();
      return;
    }

    if (ctx.state === 'suspended') {
      ctx.resume().then(() => this.notify()).catch(() => {});
      // Play 1-sample silent buffer to satisfy strict iOS Safari gesture unlock
      try {
        const silent = ctx.createBuffer(1, 1, ctx.sampleRate);
        const src = ctx.createBufferSource();
        src.buffer = silent;
        src.connect(ctx.destination);
        src.start(0);
      } catch {
        // Best-effort unlock
      }
    }
  }

  setVolume(val) {
    this.volume = Math.max(0, Math.min(1, Number(val)));
    if (this.sfxGain && this.ctx) {
      this.sfxGain.gain.setTargetAtTime(this.volume, this.ctx.currentTime, 0.015);
    }
    this.notify();
  }

  getVolume() {
    return this.volume;
  }

  setAudioSessionType(type) {
    this.sessionType = type === 'ambient' ? 'ambient' : 'playback';
    this.applyAudioSession();
    this.notify();
  }

  getAudioSessionType() {
    return this.sessionType;
  }

  getAudioStatus() {
    return {
      state: this.ctx ? this.ctx.state : 'uninitialized',
      sessionType: this.sessionType,
      volume: this.volume,
      hasAudioSessionApi: typeof navigator !== 'undefined' && 'audioSession' in navigator,
    };
  }

  getAudioLog() {
    return this.log;
  }

  emit(event) {
    const record = {
      ...event,
      ts: new Date().toLocaleTimeString('zh-CN', { hour12: false }),
    };
    if (event.type !== 'hover') {
      this.log = [record, ...this.log].slice(0, 12);
    }

    const ctx = this.ensureContext();
    if (!ctx) {
      this.notify();
      return;
    }

    if (ctx.state === 'suspended') {
      ctx.resume().catch(() => {});
    }
    if (ctx.state !== 'running') {
      this.notify();
      return;
    }

    const now = ctx.currentTime;
    switch (event.type) {
      case 'hover': {
        if (now - this.lastHoverAt < 0.065) return;
        this.lastHoverAt = now;
        this.playTileTick(ctx, now, 1650, 0.04, 0.025);
        break;
      }
      case 'draw':
        this.playDraw(ctx, now);
        break;
      case 'discard_soft':
        this.playDiscardSoft(ctx, now);
        break;
      case 'discard_slam':
        this.playDiscardSlam(ctx, now);
        break;
      case 'meld_pon':
        this.playMeldPon(ctx, now);
        break;
      case 'meld_kong':
        this.playMeldKong(ctx, now);
        break;
      case 'hu':
        this.playHu(ctx, now);
        break;
      case 'pass':
        this.playPass(ctx, now);
        break;
      default:
        break;
    }
    this.notify();
  }

  playTileTick(ctx, time, freq, gainVal, dur) {
    const osc = ctx.createOscillator();
    const g = ctx.createGain();
    osc.type = 'triangle';
    osc.frequency.setValueAtTime(freq, time);
    osc.frequency.exponentialRampToValueAtTime(freq * 0.55, time + dur);
    g.gain.setValueAtTime(gainVal, time);
    g.gain.exponentialRampToValueAtTime(0.0001, time + dur);
    osc.connect(g);
    g.connect(this.sfxGain);
    osc.start(time);
    osc.stop(time + dur + 0.01);
  }

  playNoiseBurst(ctx, time, filterFreq, q, gainVal, dur) {
    if (!this.noiseBuffer) return;
    const src = ctx.createBufferSource();
    src.buffer = this.noiseBuffer;
    const filter = ctx.createBiquadFilter();
    filter.type = 'bandpass';
    filter.frequency.setValueAtTime(filterFreq, time);
    filter.Q.value = q;
    const g = ctx.createGain();
    g.gain.setValueAtTime(gainVal, time);
    g.gain.exponentialRampToValueAtTime(0.0001, time + dur);
    src.connect(filter);
    filter.connect(g);
    g.connect(this.sfxGain);
    src.start(time);
    src.stop(time + dur + 0.01);
  }

  playDraw(ctx, now) {
    // Ivory brush slide + soft click
    this.playNoiseBurst(ctx, now, 2200, 1.4, 0.14, 0.075);
    this.playTileTick(ctx, now + 0.025, 1180, 0.14, 0.04);
  }

  playDiscardSoft(ctx, now) {
    this.playTileTick(ctx, now, 1320, 0.28, 0.055);
    this.playTileTick(ctx, now + 0.006, 260, 0.25, 0.065);
    this.playNoiseBurst(ctx, now, 1500, 1.8, 0.18, 0.04);
  }

  playDiscardSlam(ctx, now) {
    // Crisp acrylic/bone clack + resonant wooden felt thud
    this.playTileTick(ctx, now, 1880, 0.45, 0.055);
    this.playTileTick(ctx, now + 0.004, 860, 0.35, 0.07);
    this.playTileTick(ctx, now + 0.008, 175, 0.55, 0.11);
    this.playNoiseBurst(ctx, now, 2100, 1.3, 0.34, 0.06);
  }

  playMeldPon(ctx, now) {
    this.playDiscardSlam(ctx, now);
    this.playDiscardSlam(ctx, now + 0.07);
    // Brassy comic accent hit
    const osc = ctx.createOscillator();
    const g = ctx.createGain();
    osc.type = 'sawtooth';
    osc.frequency.setValueAtTime(330, now);
    osc.frequency.exponentialRampToValueAtTime(495, now + 0.09);
    const lp = ctx.createBiquadFilter();
    lp.type = 'lowpass';
    lp.frequency.value = 1800;
    g.gain.setValueAtTime(0.18, now);
    g.gain.exponentialRampToValueAtTime(0.0001, now + 0.22);
    osc.connect(lp);
    lp.connect(g);
    g.connect(this.sfxGain);
    osc.start(now);
    osc.stop(now + 0.24);
  }

  playMeldKong(ctx, now) {
    this.playDiscardSlam(ctx, now);
    this.playDiscardSlam(ctx, now + 0.06);
    this.playDiscardSlam(ctx, now + 0.12);
    // Heavy sub-bass punch + metallic ring
    const sub = ctx.createOscillator();
    const subG = ctx.createGain();
    sub.type = 'sine';
    sub.frequency.setValueAtTime(145, now);
    sub.frequency.exponentialRampToValueAtTime(46, now + 0.26);
    subG.gain.setValueAtTime(0.65, now);
    subG.gain.exponentialRampToValueAtTime(0.0001, now + 0.28);
    sub.connect(subG);
    subG.connect(this.sfxGain);
    sub.start(now);
    sub.stop(now + 0.3);

    this.playTileTick(ctx, now + 0.04, 1046, 0.24, 0.32);
  }

  playHu(ctx, now) {
    this.playDiscardSlam(ctx, now);
    // Dramatic pentatonic comic fanfare chord
    const freqs = [261.63, 329.63, 392.0, 523.25, 659.25];
    freqs.forEach((f, idx) => {
      const start = now + idx * 0.045;
      const osc = ctx.createOscillator();
      const g = ctx.createGain();
      osc.type = idx < 2 ? 'triangle' : 'sine';
      osc.frequency.setValueAtTime(f, start);
      g.gain.setValueAtTime(0.22, start);
      g.gain.exponentialRampToValueAtTime(0.0001, start + 0.55);
      osc.connect(g);
      g.connect(this.sfxGain);
      osc.start(start);
      osc.stop(start + 0.58);
    });
  }

  playPass(ctx, now) {
    this.playTileTick(ctx, now, 420, 0.16, 0.06);
  }
}

export const audioRouter = new AudioEventRouter();
