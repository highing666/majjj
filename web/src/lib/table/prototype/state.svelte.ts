// PROTOTYPE (throwaway for #78): Shared reactive state for active prototype variant.

import {
  readPrototypeVariantFromUrl,
  type PrototypeVariantId,
} from "./variants";

class PrototypeVariantStore {
  variant = $state<PrototypeVariantId>(readPrototypeVariantFromUrl());

  setVariant(next: PrototypeVariantId): void {
    this.variant = next;
  }
}

export const prototypeState = new PrototypeVariantStore();
