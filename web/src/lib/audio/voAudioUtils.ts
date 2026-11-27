import type {
  AudioBufferSourceNodeLike,
  AudioContextLike,
  AudioNodeLike,
  FetchLike,
} from "./audioTypes";

export function defaultFetch(): FetchLike | null {
  if (typeof globalThis.fetch !== "function") {
    return null;
  }
  return (input: string) => globalThis.fetch(input);
}

export function stopSourceQuietly(source: AudioBufferSourceNodeLike): void {
  source.onended = null;
  try {
    source.stop();
  } catch {
    // Ignore already-stopped source errors.
  }
}

export function connectWithRelativePan(
  ctx: AudioContextLike,
  destination: AudioNodeLike,
  pan: number,
): AudioNodeLike {
  if (pan !== 0 && typeof ctx.createStereoPanner === "function") {
    const panner = ctx.createStereoPanner();
    panner.pan.value = Math.max(-1, Math.min(1, pan));
    panner.connect(destination);
    return panner;
  }
  return destination;
}
