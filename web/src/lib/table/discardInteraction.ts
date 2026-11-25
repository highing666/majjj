import type { Tile } from "../protocol/generated/Tile";

export const DRAG_ACTIVATE_DISTANCE_PX = 8;
export const SWIPE_DISCARD_DISTANCE_PX = 48;
export const SWIPE_DISCARD_VELOCITY_PX_PER_MS = 0.45;
export const SWIPE_VELOCITY_MIN_LIFT_PX = 20;

export interface HandTileSlot {
  slotId: string;
  tile: Tile;
  fromDrawn: boolean;
  legal: boolean;
}

export interface PendingDiscard {
  slotId: string;
  tile: Tile;
  fromDrawn: boolean;
}

export interface DiscardDragState {
  slotId: string;
  tile: Tile;
  dx: number;
  dy: number;
  readyToCommit: boolean;
}

export interface DiscardInteractionState {
  confirmDiscard: boolean;
  hoveredSlotId: string | null;
  selectedSlotId: string | null;
  ghostTile: Tile | null;
  liftingSlotId: string | null;
  pendingDiscard: PendingDiscard | null;
  drag: DiscardDragState | null;
  snapBackSlotId: string | null;
  locked: boolean;
}

export interface DiscardInteractionOptions {
  confirmDiscard: boolean;
  onCommitDiscard: (tile: Tile, fromDrawn: boolean) => void;
  onTileHover?: (tile: Tile, fromDrawn: boolean) => void;
  onTileSelect?: (tile: Tile, fromDrawn: boolean) => void;
  onStateChange?: (state: DiscardInteractionState) => void;
}

export interface DiscardInteractionController {
  getState(): DiscardInteractionState;
  setConfirmDiscard(confirmDiscard: boolean): void;
  setInputLocked(locked: boolean): void;
  syncContext(context: {
    canDiscard: boolean;
    legalSlotIds?: ReadonlySet<string>;
  }): void;
  resetOnRejected(): void;
  pointerEnter(slot: HandTileSlot): void;
  pointerLeave(slotId: string): void;
  pointerDown(slot: HandTileSlot, x: number, y: number, nowMs?: number): void;
  pointerMove(x: number, y: number, nowMs?: number): void;
  pointerUp(x: number, y: number, nowMs?: number): void;
  pointerCancel(): void;
  tapBlankArea(): void;
}

interface ActivePointer {
  slot: HandTileSlot;
  startX: number;
  startY: number;
  lastY: number;
  startTimeMs: number;
  lastTimeMs: number;
  dragging: boolean;
  upwardVelocity: number;
}

function computeUpwardVelocityPxPerMs(
  startY: number,
  currentY: number,
  startAtMs: number,
  currentAtMs: number,
): number {
  const dt = Math.max(1, currentAtMs - startAtMs);
  return (startY - currentY) / dt;
}

function samplePointerUpwardVelocity(
  ptr: Pick<ActivePointer, "startY" | "lastY" | "startTimeMs" | "lastTimeMs">,
  currentY: number,
  currentAtMs: number,
): number {
  const instantUpVelocity = computeUpwardVelocityPxPerMs(
    ptr.lastY,
    currentY,
    ptr.lastTimeMs,
    currentAtMs,
  );
  const totalDt = Math.max(1, currentAtMs - ptr.startTimeMs);
  const overallUpVelocity =
    totalDt <= 250
      ? computeUpwardVelocityPxPerMs(
          ptr.startY,
          currentY,
          ptr.startTimeMs,
          currentAtMs,
        )
      : 0;
  return Math.max(instantUpVelocity, overallUpVelocity);
}

function isSwipeDiscardReady(dy: number, upwardVelocity: number): boolean {
  const upwardLift = -dy;
  if (upwardLift >= SWIPE_DISCARD_DISTANCE_PX) {
    return true;
  }
  return (
    upwardLift >= SWIPE_VELOCITY_MIN_LIFT_PX &&
    upwardVelocity >= SWIPE_DISCARD_VELOCITY_PX_PER_MS
  );
}

/**
 * Pure TypeScript interaction state machine for desktop single-click (`confirm_discard = false`)
 * and touch two-tap / upward-swipe (`confirm_discard = true`) tile discarding.
 */
export function createDiscardInteractionController(
  options: DiscardInteractionOptions,
): DiscardInteractionController {
  let confirmDiscard = options.confirmDiscard;
  let externalLocked = false;
  let hoveredSlotId: string | null = null;
  let selectedSlotId: string | null = null;
  let ghostTile: Tile | null = null;
  let liftingSlotId: string | null = null;
  let pendingDiscard: PendingDiscard | null = null;
  let drag: DiscardDragState | null = null;
  let snapBackSlotId: string | null = null;
  let activePointer: ActivePointer | null = null;

  function isLocked(): boolean {
    return externalLocked || pendingDiscard !== null;
  }

  function snapshotState(): DiscardInteractionState {
    return {
      confirmDiscard,
      hoveredSlotId,
      selectedSlotId,
      ghostTile,
      liftingSlotId,
      pendingDiscard: pendingDiscard ? { ...pendingDiscard } : null,
      drag: drag ? { ...drag } : null,
      snapBackSlotId,
      locked: isLocked(),
    };
  }

  function emitState(): void {
    options.onStateChange?.(snapshotState());
  }

  function clearEphemeralPreview(): void {
    hoveredSlotId = null;
    selectedSlotId = null;
    ghostTile = null;
    drag = null;
    activePointer = null;
  }

  function commitSlot(slot: HandTileSlot): void {
    clearEphemeralPreview();
    snapBackSlotId = null;
    pendingDiscard = {
      slotId: slot.slotId,
      tile: slot.tile,
      fromDrawn: slot.fromDrawn,
    };
    liftingSlotId = slot.slotId;
    emitState();
    options.onCommitDiscard(slot.tile, slot.fromDrawn);
  }

  function selectSlot(slot: HandTileSlot): void {
    const changed = selectedSlotId !== slot.slotId;
    selectedSlotId = slot.slotId;
    ghostTile = slot.tile;
    snapBackSlotId = null;
    emitState();
    if (changed) {
      options.onTileSelect?.(slot.tile, slot.fromDrawn);
    }
  }

  function resetAll(): void {
    clearEphemeralPreview();
    pendingDiscard = null;
    liftingSlotId = null;
    snapBackSlotId = null;
    emitState();
  }

  return {
    getState: snapshotState,

    setConfirmDiscard(next: boolean): void {
      if (confirmDiscard === next) {
        return;
      }
      confirmDiscard = next;
      clearEphemeralPreview();
      snapBackSlotId = null;
      emitState();
    },

    setInputLocked(locked: boolean): void {
      const wasLocked = externalLocked;
      externalLocked = locked;
      if (locked) {
        activePointer = null;
        drag = null;
        hoveredSlotId = null;
        if (pendingDiscard === null) {
          selectedSlotId = null;
          ghostTile = null;
        }
        emitState();
        return;
      }
      if (wasLocked || pendingDiscard !== null) {
        pendingDiscard = null;
        liftingSlotId = null;
        clearEphemeralPreview();
        snapBackSlotId = null;
        emitState();
      }
    },

    syncContext({ canDiscard, legalSlotIds }): void {
      if (!canDiscard) {
        if (
          hoveredSlotId !== null ||
          selectedSlotId !== null ||
          ghostTile !== null ||
          drag !== null ||
          activePointer !== null ||
          (!externalLocked && pendingDiscard !== null)
        ) {
          clearEphemeralPreview();
          if (!externalLocked) {
            pendingDiscard = null;
            liftingSlotId = null;
          }
          emitState();
        }
        return;
      }
      if (legalSlotIds) {
        let changed = false;
        if (hoveredSlotId !== null && !legalSlotIds.has(hoveredSlotId)) {
          hoveredSlotId = null;
          if (selectedSlotId === null) {
            ghostTile = null;
          }
          changed = true;
        }
        if (selectedSlotId !== null && !legalSlotIds.has(selectedSlotId)) {
          selectedSlotId = null;
          ghostTile = null;
          changed = true;
        }
        if (changed) {
          emitState();
        }
      }
    },

    resetOnRejected(): void {
      externalLocked = false;
      resetAll();
    },

    pointerEnter(slot: HandTileSlot): void {
      if (confirmDiscard || isLocked() || !slot.legal) {
        return;
      }
      if (hoveredSlotId === slot.slotId && ghostTile === slot.tile) {
        return;
      }
      hoveredSlotId = slot.slotId;
      ghostTile = slot.tile;
      snapBackSlotId = null;
      emitState();
      options.onTileHover?.(slot.tile, slot.fromDrawn);
    },

    pointerLeave(slotId: string): void {
      if (confirmDiscard || hoveredSlotId !== slotId) {
        return;
      }
      hoveredSlotId = null;
      ghostTile = null;
      emitState();
    },

    pointerDown(
      slot: HandTileSlot,
      x: number,
      y: number,
      nowMs = Date.now(),
    ): void {
      if (isLocked() || !slot.legal) {
        return;
      }
      snapBackSlotId = null;
      activePointer = {
        slot,
        startX: x,
        startY: y,
        lastY: y,
        startTimeMs: nowMs,
        lastTimeMs: nowMs,
        dragging: false,
        upwardVelocity: 0,
      };
    },

    pointerMove(x: number, y: number, nowMs = Date.now()): void {
      if (!activePointer || isLocked()) {
        return;
      }
      const dx = x - activePointer.startX;
      const dy = y - activePointer.startY;
      activePointer.upwardVelocity = samplePointerUpwardVelocity(
        activePointer,
        y,
        nowMs,
      );
      activePointer.lastY = y;
      activePointer.lastTimeMs = nowMs;

      if (
        !activePointer.dragging &&
        Math.hypot(dx, dy) > DRAG_ACTIVATE_DISTANCE_PX
      ) {
        activePointer.dragging = true;
        if (confirmDiscard && selectedSlotId !== activePointer.slot.slotId) {
          selectSlot(activePointer.slot);
        }
      }

      if (!activePointer.dragging) {
        return;
      }

      const readyToCommit = isSwipeDiscardReady(
        dy,
        activePointer.upwardVelocity,
      );
      drag = {
        slotId: activePointer.slot.slotId,
        tile: activePointer.slot.tile,
        dx,
        dy,
        readyToCommit,
      };
      ghostTile = activePointer.slot.tile;
      emitState();
    },

    pointerUp(_x: number, y: number, nowMs = Date.now()): void {
      const ptr = activePointer;
      if (!ptr) {
        return;
      }
      activePointer = null;
      if (isLocked()) {
        drag = null;
        emitState();
        return;
      }

      const dy = y - ptr.startY;
      const upwardVelocity = Math.max(
        ptr.upwardVelocity,
        samplePointerUpwardVelocity(ptr, y, nowMs),
      );

      if (ptr.dragging) {
        drag = null;
        if (isSwipeDiscardReady(dy, upwardVelocity)) {
          commitSlot(ptr.slot);
        } else {
          selectedSlotId = null;
          ghostTile = null;
          snapBackSlotId = ptr.slot.slotId;
          emitState();
        }
        return;
      }

      if (!confirmDiscard) {
        commitSlot(ptr.slot);
        return;
      }

      if (selectedSlotId === ptr.slot.slotId) {
        commitSlot(ptr.slot);
      } else {
        selectSlot(ptr.slot);
      }
    },

    pointerCancel(): void {
      if (!activePointer) {
        return;
      }
      const slotId = activePointer.slot.slotId;
      const wasDragging = activePointer.dragging;
      activePointer = null;
      drag = null;
      if (wasDragging) {
        selectedSlotId = null;
        ghostTile = null;
        snapBackSlotId = slotId;
      }
      emitState();
    },

    tapBlankArea(): void {
      if (
        isLocked() ||
        (selectedSlotId === null && hoveredSlotId === null && ghostTile === null)
      ) {
        return;
      }
      clearEphemeralPreview();
      snapBackSlotId = null;
      emitState();
    },
  };
}
