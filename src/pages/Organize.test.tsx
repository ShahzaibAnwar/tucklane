import { describe, expect, it } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Tooltip } from "radix-ui";
import { Organize } from "./Organize";
import { appStatus, mockBackend, plan, planItem, seedStore } from "@/test/fixtures";

function renderPage() {
  return render(
    <Tooltip.Provider>
      <Organize />
    </Tooltip.Provider>,
  );
}

describe("Organize preview", () => {
  it("shows the real plan, lets the user exclude items and executes only the selected ones", async () => {
    seedStore(appStatus());
    const p = plan([
      planItem({ name: "report.pdf" }),
      planItem({ name: "setup.exe", destDir: "Software/Installers" }),
      planItem({
        name: "invoice.pdf",
        status: "renamed",
        destName: "invoice (1).pdf",
        note: "already exists",
      }),
      planItem({
        name: "big.zip.crdownload",
        status: "pending",
        selected: false,
        destDir: "Archives",
      }),
    ]);
    const calls = mockBackend({
      preview_organization: () => p,
      list_saved_plans: () => [],
      execute_organization: (a) => ({
        session: {
          id: "s1",
          kind: "manual",
          status: "completed",
          startedAt: "",
          endedAt: "",
          total: (a.itemIds as string[]).length,
          completed: (a.itemIds as string[]).length,
          skipped: 0,
          failed: 0,
          undoneAt: null,
          undoable: 2,
        },
        cancelled: false,
        errors: [],
      }),
    });
    const user = userEvent.setup();
    renderPage();

    expect(await screen.findByText("report.pdf")).toBeInTheDocument();
    expect(screen.getByText(/as invoice \(1\)\.pdf/)).toBeInTheDocument();
    // Pending downloads are not in the proposed view.
    expect(screen.queryByText("big.zip.crdownload")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Organize Selected \(3\)/ })).toBeEnabled();

    await user.click(screen.getByRole("checkbox", { name: "Include setup.exe" }));
    await user.click(screen.getByRole("button", { name: /Organize Selected \(2\)/ }));

    await waitFor(() => expect(calls.some((c) => c.cmd === "execute_organization")).toBe(true));
    const exec = calls.find((c) => c.cmd === "execute_organization")!;
    expect(exec.args.planId).toBe("plan-1");
    expect((exec.args.itemIds as string[]).sort()).toEqual(["invoice.pdf", "report.pdf"]);
    expect(await screen.findByRole("button", { name: /Undo This Session/ })).toBeInTheDocument();
  });

  it("shows an empty state when there is nothing to organize", async () => {
    seedStore(appStatus());
    mockBackend({ preview_organization: () => plan([]), list_saved_plans: () => [] });
    renderPage();
    expect(await screen.findByText("Nothing to organize")).toBeInTheDocument();
  });
});
