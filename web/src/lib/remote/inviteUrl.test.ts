import { describe, expect, it, vi } from "vitest";
import {
  clearInviteCodeFromLocation,
  stripInviteCodeFromUrl,
} from "./inviteUrl";

describe("inviteUrl", () => {
  it("strips ?code=... while preserving other query parameters and hash fragments", () => {
    expect(
      stripInviteCodeFromUrl("http://majjj.example.com:8789/?code=ABCD2345"),
    ).toBe("/");

    expect(
      stripInviteCodeFromUrl(
        "http://[2001:db8::1]:8789/?foo=1&code=ABCD2345&bar=2#table",
      ),
    ).toBe("/?foo=1&bar=2#table");

    expect(
      stripInviteCodeFromUrl("http://majjj.example.com:8789/?code=A&code=B"),
    ).toBe("/");
  });

  it("returns null when URL does not contain a code query parameter or is invalid", () => {
    expect(
      stripInviteCodeFromUrl("http://majjj.example.com:8789/"),
    ).toBeNull();
    expect(
      stripInviteCodeFromUrl("http://majjj.example.com:8789/?foo=bar#hash"),
    ).toBeNull();
    expect(stripInviteCodeFromUrl("not-a-valid-url")).toBeNull();
  });

  it("clears ?code=... from window.location via history.replaceState on startup", () => {
    const replaceState = vi.fn();
    const removed = clearInviteCodeFromLocation({
      location: { href: "http://majjj.example.com:8789/?code=ABCD2345&ref=wx#top" },
      history: { replaceState },
    });
    expect(removed).toBe(true);
    expect(replaceState).toHaveBeenCalledTimes(1);
    expect(replaceState).toHaveBeenCalledWith(null, "", "/?ref=wx#top");

    const replaceStateNoop = vi.fn();
    const skipped = clearInviteCodeFromLocation({
      location: { href: "http://majjj.example.com:8789/" },
      history: { replaceState: replaceStateNoop },
    });
    expect(skipped).toBe(false);
    expect(replaceStateNoop).not.toHaveBeenCalled();
  });
});
