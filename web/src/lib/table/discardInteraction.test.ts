import { describe, expect, it, vi } from "vitest";
import {
  DRAG_ACTIVATE_DISTANCE_PX,
  SWIPE_DISCARD_DISTANCE_PX,
  createDiscardInteractionController,
  type HandTileSlot,
} from "./discardInteraction";

const TILE_3M = 2;
const TILE_5P = 22;
const TILE_9S = 17;

const CLOSED_3M: HandTileSlot = {
  slotId: "closed-0",
  tile: TILE_3M,
  fromDrawn: false,
  legal: true,
};

const CLOSED_5P: HandTileSlot = {
  slotId: "closed-1",
  tile: TILE_5P,
  fromDrawn: false,
  legal: true,
};

const DRAWN_3M: HandTileSlot = {
  slotId: "drawn",
  tile: TILE_3M,
  fromDrawn: true,
  legal: true,
};

const ILLEGAL_9S: HandTileSlot = {
  slotId: "closed-2",
  tile: TILE_9S,
  fromDrawn: false,
  legal: false,
};

describe("discardInteraction", () => {
  it("Mac fine-pointer (confirmDiscard=false): hovers with ghost preview, single click commits exact from_drawn and locks against double-clicks", () => {
    const onCommitDiscard = vi.fn();
    const onTileHover = vi.fn();
    const onTileSelect = vi.fn();

    const controller = createDiscardInteractionController({
      confirmDiscard: false,
      onCommitDiscard,
      onTileHover,
      onTileSelect,
    });

    controller.pointerEnter(CLOSED_3M);
    expect(controller.getState().hoveredSlotId).toBe("closed-0");
    expect(controller.getState().ghostTile).toBe(TILE_3M);
    expect(onTileHover).toHaveBeenCalledTimes(1);
    expect(onTileHover).toHaveBeenCalledWith(TILE_3M, false);

    // Switching hover to drawn tile with the same tile face preserves distinct slotId and fromDrawn=true
    controller.pointerEnter(DRAWN_3M);
    expect(controller.getState().hoveredSlotId).toBe("drawn");
    expect(controller.getState().ghostTile).toBe(TILE_3M);
    expect(onTileHover).toHaveBeenNthCalledWith(2, TILE_3M, true);

    controller.pointerLeave("drawn");
    expect(controller.getState().hoveredSlotId).toBeNull();
    expect(controller.getState().ghostTile).toBeNull();

    // Single click commits immediately and locks input
    controller.pointerEnter(DRAWN_3M);
    controller.pointerDown(DRAWN_3M, 200, 520, 1000);
    controller.pointerUp(201, 520, 1050);

    expect(onCommitDiscard).toHaveBeenCalledTimes(1);
    expect(onCommitDiscard).toHaveBeenCalledWith(TILE_3M, true);
    expect(controller.getState().locked).toBe(true);
    expect(controller.getState().liftingSlotId).toBe("drawn");
    expect(controller.getState().ghostTile).toBeNull();

    // Second click while locked is ignored
    controller.pointerDown(CLOSED_5P, 120, 520, 1080);
    controller.pointerUp(120, 520, 1100);
    expect(onCommitDiscard).toHaveBeenCalledTimes(1);
    expect(onTileSelect).not.toHaveBeenCalled();
  });

  it("resets cleanly on ActionRejected or external inputLocked release without mutating river state", () => {
    const onCommitDiscard = vi.fn();
    const controller = createDiscardInteractionController({
      confirmDiscard: false,
      onCommitDiscard,
    });

    controller.pointerDown(CLOSED_5P, 100, 500, 10);
    controller.pointerUp(100, 500, 40);
    expect(controller.getState().locked).toBe(true);
    expect(controller.getState().pendingDiscard).toEqual({
      slotId: "closed-1",
      tile: TILE_5P,
      fromDrawn: false,
    });

    controller.setInputLocked(true);
    controller.setInputLocked(false);
    expect(controller.getState().locked).toBe(false);
    expect(controller.getState().pendingDiscard).toBeNull();
    expect(controller.getState().liftingSlotId).toBeNull();

    // Also supports explicit resetOnRejected
    controller.pointerDown(CLOSED_3M, 100, 500, 100);
    controller.pointerUp(100, 500, 120);
    expect(controller.getState().locked).toBe(true);

    controller.resetOnRejected();
    expect(controller.getState().locked).toBe(false);
    expect(controller.getState().liftingSlotId).toBeNull();
  });

  it("iPhone touch (confirmDiscard=true): first tap selects with ghost, tap other switches, tap blank cancels, second tap same tile commits", () => {
    const onCommitDiscard = vi.fn();
    const onTileHover = vi.fn();
    const onTileSelect = vi.fn();

    const controller = createDiscardInteractionController({
      confirmDiscard: true,
      onCommitDiscard,
      onTileHover,
      onTileSelect,
    });

    // Hover events are ignored in confirmDiscard=true mode
    controller.pointerEnter(CLOSED_3M);
    expect(controller.getState().hoveredSlotId).toBeNull();
    expect(onTileHover).not.toHaveBeenCalled();

    // First tap selects closed-0
    controller.pointerDown(CLOSED_3M, 80, 500, 100);
    controller.pointerUp(80, 500, 140);
    expect(controller.getState().selectedSlotId).toBe("closed-0");
    expect(controller.getState().ghostTile).toBe(TILE_3M);
    expect(onTileSelect).toHaveBeenCalledTimes(1);
    expect(onTileSelect).toHaveBeenCalledWith(TILE_3M, false);
    expect(onCommitDiscard).not.toHaveBeenCalled();

    // Tap other tile switches selection
    controller.pointerDown(CLOSED_5P, 130, 500, 200);
    controller.pointerUp(130, 500, 240);
    expect(controller.getState().selectedSlotId).toBe("closed-1");
    expect(controller.getState().ghostTile).toBe(TILE_5P);
    expect(onTileSelect).toHaveBeenCalledTimes(2);
    expect(onTileSelect).toHaveBeenLastCalledWith(TILE_5P, false);
    expect(onCommitDiscard).not.toHaveBeenCalled();

    // Tap blank area cancels selection
    controller.tapBlankArea();
    expect(controller.getState().selectedSlotId).toBeNull();
    expect(controller.getState().ghostTile).toBeNull();

    // First tap on drawn tile, then second tap on drawn tile commits with fromDrawn=true
    controller.pointerDown(DRAWN_3M, 300, 500, 300);
    controller.pointerUp(300, 500, 340);
    expect(controller.getState().selectedSlotId).toBe("drawn");
    expect(onCommitDiscard).not.toHaveBeenCalled();

    controller.pointerDown(DRAWN_3M, 300, 500, 420);
    controller.pointerUp(300, 500, 460);
    expect(onCommitDiscard).toHaveBeenCalledTimes(1);
    expect(onCommitDiscard).toHaveBeenCalledWith(TILE_3M, true);
    expect(controller.getState().locked).toBe(true);
    expect(controller.getState().selectedSlotId).toBeNull();
  });

  it("iPhone touch drag upward: commits past displacement or velocity threshold, snaps back below threshold, ignores illegal tiles", () => {
    const onCommitDiscard = vi.fn();
    const onTileSelect = vi.fn();

    const controller = createDiscardInteractionController({
      confirmDiscard: true,
      onCommitDiscard,
      onTileSelect,
    });

    // Illegal tile does nothing
    controller.pointerDown(ILLEGAL_9S, 200, 500, 0);
    controller.pointerUp(200, 400, 50);
    expect(onCommitDiscard).not.toHaveBeenCalled();
    expect(onTileSelect).not.toHaveBeenCalled();

    // Drag slowly below displacement threshold -> snaps back without committing
    controller.pointerDown(CLOSED_5P, 150, 500, 100);
    controller.pointerMove(150, 500 - (DRAG_ACTIVATE_DISTANCE_PX + 4), 250);
    expect(controller.getState().drag).not.toBeNull();
    expect(controller.getState().drag?.readyToCommit).toBe(false);
    expect(controller.getState().ghostTile).toBe(TILE_5P);
    expect(onTileSelect).toHaveBeenCalledTimes(1);

    controller.pointerMove(150, 500 - (SWIPE_DISCARD_DISTANCE_PX - 10), 600);
    expect(controller.getState().drag?.readyToCommit).toBe(false);

    controller.pointerUp(150, 500 - (SWIPE_DISCARD_DISTANCE_PX - 10), 750);
    expect(onCommitDiscard).not.toHaveBeenCalled();
    expect(controller.getState().drag).toBeNull();
    expect(controller.getState().snapBackSlotId).toBe("closed-1");
    expect(controller.getState().selectedSlotId).toBeNull();
    expect(controller.getState().ghostTile).toBeNull();

    // Drag upward past displacement threshold -> commits
    controller.pointerDown(CLOSED_3M, 100, 500, 1000);
    controller.pointerMove(100, 480, 1040);
    controller.pointerMove(100, 500 - SWIPE_DISCARD_DISTANCE_PX - 4, 1120);
    expect(controller.getState().drag?.readyToCommit).toBe(true);
    controller.pointerUp(100, 500 - SWIPE_DISCARD_DISTANCE_PX - 4, 1140);
    expect(onCommitDiscard).toHaveBeenCalledTimes(1);
    expect(onCommitDiscard).toHaveBeenCalledWith(TILE_3M, false);

    // Unlock and test quick upward flick (velocity threshold with >= 20px lift)
    controller.resetOnRejected();
    controller.pointerDown(DRAWN_3M, 300, 500, 2000);
    controller.pointerMove(300, 488, 2010);
    controller.pointerMove(300, 472, 2025); // 16px in 15ms => 1.07 px/ms, total lift 28px >= 20px
    expect(controller.getState().drag?.readyToCommit).toBe(true);
    controller.pointerUp(300, 472, 2030);
    expect(onCommitDiscard).toHaveBeenCalledTimes(2);
    expect(onCommitDiscard).toHaveBeenLastCalledWith(TILE_3M, true);
  });

  it("clears ephemeral selection/hover when confirmDiscard flips, turn ends, or pointer is cancelled", () => {
    const onCommitDiscard = vi.fn();
    const controller = createDiscardInteractionController({
      confirmDiscard: true,
      onCommitDiscard,
    });

    controller.pointerDown(CLOSED_3M, 100, 500, 0);
    controller.pointerUp(100, 500, 30);
    expect(controller.getState().selectedSlotId).toBe("closed-0");
    expect(controller.getState().ghostTile).toBe(TILE_3M);

    // Switching mode mid-turn clears active selection & ghost
    controller.setConfirmDiscard(false);
    expect(controller.getState().selectedSlotId).toBeNull();
    expect(controller.getState().ghostTile).toBeNull();

    // Hover then turn ends -> clears hover & ghost
    controller.pointerEnter(CLOSED_5P);
    expect(controller.getState().hoveredSlotId).toBe("closed-1");
    controller.syncContext({ canDiscard: false });
    expect(controller.getState().hoveredSlotId).toBeNull();
    expect(controller.getState().ghostTile).toBeNull();

    // Pointer cancel mid-drag aborts without committing
    controller.syncContext({ canDiscard: true });
    controller.setConfirmDiscard(true);
    controller.pointerDown(DRAWN_3M, 300, 500, 100);
    controller.pointerMove(300, 440, 130);
    expect(controller.getState().drag).not.toBeNull();
    controller.pointerCancel();
    expect(controller.getState().drag).toBeNull();
    expect(controller.getState().snapBackSlotId).toBe("drawn");
    expect(onCommitDiscard).not.toHaveBeenCalled();
  });
});
