import { describe, expect, it } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { UndoDialogHost, undoOperations } from "./undo";
import { Toaster } from "@/components/ui/toast";
import { appStatus, mockBackend, seedStore } from "@/test/fixtures";

describe("undo flow", () => {
  it("never replaces a file silently: asks, then restores under a new name", async () => {
    seedStore(appStatus());
    const calls = mockBackend({
      get_app_status: () => appStatus(),
      undo_operations: (a) => {
        const opts = a.options as { onConflict?: string } | null;
        return opts?.onConflict === "rename"
          ? {
              sessionId: "u2",
              results: [
                {
                  operationId: "op1",
                  name: "a.pdf",
                  status: "restored",
                  message: 'Restored as "a (1).pdf" because the original name was taken.',
                  restoredRel: "a (1).pdf",
                },
              ],
            }
          : {
              sessionId: "u1",
              results: [
                {
                  operationId: "op1",
                  name: "a.pdf",
                  status: "conflict",
                  message: "taken",
                  restoredRel: null,
                },
              ],
            };
      },
    });
    const user = userEvent.setup();
    render(
      <>
        <UndoDialogHost />
        <Toaster />
      </>,
    );
    await undoOperations(["op1"]);
    expect(await screen.findByText("The original name is taken")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Restore with a new name" }));
    await waitFor(() => expect(calls.filter((c) => c.cmd === "undo_operations")).toHaveLength(2));
    expect(
      calls[calls.length - 1].cmd === "undo_operations" ||
        calls.some((c) => (c.args.options as { onConflict?: string })?.onConflict === "rename"),
    ).toBe(true);
    expect(await screen.findByText("1 file restored")).toBeInTheDocument();
  });
});
