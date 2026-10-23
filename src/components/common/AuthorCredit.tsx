import { Github, Globe, Heart, Linkedin, Mail } from "lucide-react";
import { api, errorMessage } from "@/lib/api";
import { cn } from "@/lib/cn";
import { toast } from "@/components/ui/toast";

type Link = Parameters<typeof api.openAboutLink>[0];

const LINKS: { link: Link; label: string; text: string; icon: typeof Globe }[] = [
  { link: "github", label: "GitHub", text: "github.com/ShahzaibAnwar", icon: Github },
  { link: "linkedin", label: "LinkedIn", text: "linkedin.com/in/shahzaibanwar", icon: Linkedin },
  { link: "website", label: "Website", text: "dareecha.pk", icon: Globe },
  { link: "email", label: "Email", text: "shahzaib@dareecha.pk", icon: Mail },
];

export function openAboutLink(link: Link) {
  void api.openAboutLink(link).catch((e) => toast.error("Could not open link", errorMessage(e)));
}

export function CreatedBy({ className }: { className?: string }) {
  return (
    <p className={cn("flex items-center gap-1.5 text-[13px] text-muted", className)}>
      Created with
      <Heart className="size-3.5 fill-[#e0475b] text-[#e0475b]" aria-label="love" />
      by <span className="font-semibold text-fg">Shahzaib Anwar</span>
    </p>
  );
}

/** Author links. `compact` renders icon-only buttons. */
export function AuthorLinks({ compact }: { compact?: boolean }) {
  return (
    <div className={cn("flex flex-wrap", compact ? "gap-1" : "gap-2")}>
      {LINKS.map((l) => (
        <button
          key={l.link}
          onClick={() => openAboutLink(l.link)}
          aria-label={`${l.label}: ${l.text}`}
          title={l.text}
          className={cn(
            "inline-flex items-center gap-1.5 rounded-lg border border-border text-[12.5px] text-muted transition-colors hover:border-accent hover:bg-accent-soft hover:text-fg",
            compact ? "p-1.5" : "px-2.5 py-1.5",
          )}
        >
          <l.icon className="size-3.5" aria-hidden />
          {!compact && l.text}
        </button>
      ))}
    </div>
  );
}
