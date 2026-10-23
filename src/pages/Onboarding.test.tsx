import { describe, expect, it } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Tooltip } from "radix-ui";
import { Onboarding } from "./Onboarding";
import { appStatus, defaultSettings, mockBackend, seedStore } from "@/test/fixtures";

const fresh = appStatus({
  root: null,
  settings: { ...defaultSettings, onboardingComplete: false },
});

describe("Onboarding", () => {
  it("walks through folder authorization with safe defaults", async () => {
    seedStore(fresh);
    const calls = mockBackend({
      get_presets: () => [{ id: "balanced", folders: ["Documents/PDF", "Media/Images"] }],
      get_default_downloads_folder: () => "C:\\Users\\Test\\Downloads",
      validate_authorized_root: (a) => ({ ok: true, path: a.path, message: null }),
      select_authorized_root: (a) => ({ id: "r1", path: a.path, selectedAt: "" }),
      get_app_status: () =>
        appStatus({ settings: { ...defaultSettings, onboardingComplete: false } }),
      update_settings: (a) => a.settings,
      scan_downloads: () => ({
        planId: "p",
        totalFiles: 12,
        totalBytes: 2048,
        categories: 3,
        suggestedChanges: 9,
        unclassified: 1,
        inUse: 0,
        pending: 2,
        possibleDuplicateGroups: 1,
        byFolder: [["Documents/PDF", 4]],
      }),
    });
    const user = userEvent.setup();
    render(
      <Tooltip.Provider>
        <Onboarding />
      </Tooltip.Provider>,
    );

    expect(screen.getByText(/Meet Tucklane/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /Get Started/ }));
    expect(await screen.findByText("C:\\Users\\Test\\Downloads")).toBeInTheDocument();
    expect(
      screen.getByText(/only organize files inside the folder you approve/),
    ).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: /Authorize this folder/ }));
    await waitFor(() =>
      expect(calls.find((c) => c.cmd === "select_authorized_root")?.args.path).toBe(
        "C:\\Users\\Test\\Downloads",
      ),
    );

    expect(await screen.findByRole("radio", { name: /Balanced/ })).toHaveAttribute(
      "aria-checked",
      "true",
    );
    await user.click(screen.getByRole("button", { name: /Continue/ }));

    // Defaults: preview on, auto-organize off, startup off, notifications on.
    expect(screen.getByRole("switch", { name: "Preview changes before organizing" })).toBeChecked();
    expect(
      screen.getByRole("switch", { name: "Organize new downloads automatically" }),
    ).not.toBeChecked();
    expect(screen.getByRole("switch", { name: "Start with Windows" })).not.toBeChecked();
    expect(screen.getByRole("switch", { name: "Show desktop notifications" })).toBeChecked();

    await user.click(screen.getByRole("button", { name: /Analyze my folder/ }));
    expect(await screen.findByText("Suggested moves")).toBeInTheDocument();
    expect(screen.getByText("12")).toBeInTheDocument();
    // Nothing was moved during analysis.
    expect(calls.some((c) => c.cmd === "execute_organization")).toBe(false);
  });

  it("explains why a folder is rejected", async () => {
    seedStore(fresh);
    mockBackend({
      get_presets: () => [],
      get_default_downloads_folder: () => "C:\\",
      validate_authorized_root: () => ({
        ok: false,
        path: "C:\\",
        message: "A whole drive cannot be managed.",
      }),
    });
    const user = userEvent.setup();
    render(
      <Tooltip.Provider>
        <Onboarding />
      </Tooltip.Provider>,
    );
    await user.click(screen.getByRole("button", { name: /Get Started/ }));
    expect(await screen.findByRole("alert")).toHaveTextContent("A whole drive cannot be managed.");
    expect(screen.getByRole("button", { name: /Authorize this folder/ })).toBeDisabled();
  });
});
