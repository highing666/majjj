import { Filter } from "pixi.js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  createPaperGrainFilter,
  DEFAULT_PAPER_GRAIN_OPTIONS,
} from "./paperGrainFilter";

describe("createPaperGrainFilter", () => {
  beforeEach(() => {
    vi.stubGlobal("document", {
      createElement: () => ({
        getContext: () => null,
      }),
    });
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("builds a single-pass PixiJS v8 Filter with paper grain & cool-indigo vignette uniforms", () => {
    const filter = createPaperGrainFilter();
    expect(filter).toBeInstanceOf(Filter);
    expect(filter.enabled).toBe(true);
    expect(filter.clipToViewport).toBe(true);
    expect(filter.glProgram).toBeDefined();
    expect(filter.glProgram?.fragment).toContain("vNormalizedCoord");
    expect(filter.glProgram?.fragment).toContain("uGrainStrength");
    expect(filter.glProgram?.fragment).toContain("uVignetteStrength");

    const uniforms = filter.resources.paperGrainUniforms.uniforms;
    expect(uniforms.uGrainStrength).toBeCloseTo(
      DEFAULT_PAPER_GRAIN_OPTIONS.grainStrength,
      5,
    );
    expect(uniforms.uVignetteStrength).toBeCloseTo(
      DEFAULT_PAPER_GRAIN_OPTIONS.vignetteStrength,
      5,
    );
    expect(uniforms.uSeed).toBeCloseTo(DEFAULT_PAPER_GRAIN_OPTIONS.seed, 5);

    filter.destroy();
  });

  it("supports custom strength, seed, and enabled toggle without rebuilding program", () => {
    const filter = createPaperGrainFilter({
      grainStrength: 0.02,
      vignetteStrength: 0.18,
      seed: 42.5,
      enabled: false,
    });

    expect(filter.enabled).toBe(false);
    expect(
      filter.resources.paperGrainUniforms.uniforms.uGrainStrength,
    ).toBeCloseTo(0.02, 5);
    expect(
      filter.resources.paperGrainUniforms.uniforms.uVignetteStrength,
    ).toBeCloseTo(0.18, 5);
    expect(filter.resources.paperGrainUniforms.uniforms.uSeed).toBeCloseTo(
      42.5,
      5,
    );

    filter.enabled = true;
    expect(filter.enabled).toBe(true);
    filter.destroy();
  });
});
