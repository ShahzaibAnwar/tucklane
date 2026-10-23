import { describe, expect, it } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { AuthorLinks, CreatedBy } from "./AuthorCredit";
import { mockBackend } from "@/test/fixtures";

describe("author credit", () => {
  it("credits the author and opens links only by fixed name", async () => {
    const calls = mockBackend({ open_about_link: () => null });
    const user = userEvent.setup();
    render(
      <>
        <CreatedBy />
        <AuthorLinks />
      </>,
    );
    expect(screen.getByText("Shahzaib Anwar")).toBeInTheDocument();
    for (const name of ["GitHub", "LinkedIn", "Website", "Email"]) {
      expect(screen.getByRole("button", { name: new RegExp(`^${name}:`) })).toBeInTheDocument();
    }
    await user.click(screen.getByRole("button", { name: /^LinkedIn:/ }));
    await waitFor(() =>
      expect(calls.find((c) => c.cmd === "open_about_link")?.args.link).toBe("linkedin"),
    );
  });
});
