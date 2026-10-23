import { Dialog } from "@/components/ui/dialog";
import { Kbd } from "@/components/ui/primitives";
import { AuthorLinks, CreatedBy } from "@/components/common/AuthorCredit";
import { NAV } from "./nav";

export function HelpDialog({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (v: boolean) => void;
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange} title="How Tucklane works" wide>
      <div className="space-y-5 text-[13.5px] leading-relaxed">
        <section>
          <h3 className="font-semibold">Your folder, your control</h3>
          <p className="text-muted">
            Tucklane only works inside the one folder you approved. It sorts loose files at the top
            of that folder into category folders, never touches folders you created, never
            overwrites a file, and never deletes anything. Every move is recorded and can be undone
            from Activity.
          </p>
        </section>
        <section>
          <h3 className="font-semibold">How files are classified</h3>
          <ol className="list-decimal space-y-0.5 pl-5 text-muted">
            <li>Files you excluded, and protected filename patterns, are left alone.</li>
            <li>Your rules, in priority order.</li>
            <li>File signatures (only if you turned on content signatures).</li>
            <li>Filename hints such as “invoice” (only if turned on).</li>
            <li>The file extension.</li>
            <li>Anything else is left for you to review.</li>
          </ol>
        </section>
        <section>
          <h3 className="font-semibold">New downloads</h3>
          <p className="text-muted">
            While Tucklane is running (including in the notification area), it watches for new
            files. A file is only considered finished when it is no longer a temporary download, has
            stopped changing for the settling time, and is not open in another program. Monitoring
            stops when you quit Tucklane.
          </p>
        </section>
        <section>
          <h3 className="mb-1.5 font-semibold">Keyboard shortcuts</h3>
          <div className="grid grid-cols-2 gap-x-6 gap-y-1.5 text-muted">
            {NAV.map((n, i) => (
              <div key={n.page} className="flex items-center justify-between">
                <span>{n.label}</span>
                <span>
                  <Kbd>Ctrl</Kbd> <Kbd>{i + 1}</Kbd>
                </span>
              </div>
            ))}
            <div className="flex items-center justify-between">
              <span>Scan now</span>
              <span>
                <Kbd>F5</Kbd>
              </span>
            </div>
          </div>
        </section>
        <section className="flex flex-wrap items-center justify-between gap-3 border-t border-border pt-4">
          <CreatedBy />
          <AuthorLinks compact />
        </section>
      </div>
    </Dialog>
  );
}
