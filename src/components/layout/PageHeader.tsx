import type { ReactNode } from "react";

export function PageHeader({
  title,
  subtitle,
  actions,
}: {
  title: string;
  subtitle?: ReactNode;
  actions?: ReactNode;
}) {
  return (
    <header className="flex flex-wrap items-end justify-between gap-4 px-8 pt-7 pb-5">
      <div className="min-w-0">
        <h1 className="font-display text-[24px] leading-tight font-semibold tracking-tight">
          {title}
        </h1>
        {subtitle && <p className="mt-1 text-[13.5px] text-muted">{subtitle}</p>}
      </div>
      {actions && <div className="flex flex-wrap items-center gap-2">{actions}</div>}
    </header>
  );
}

export function PageBody({ children }: { children: ReactNode }) {
  return <div className="tl-enter flex flex-col gap-5 px-8 pb-10">{children}</div>;
}
