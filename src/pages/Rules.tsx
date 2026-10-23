import { useEffect, useState } from "react";
import {
  ArrowDown,
  ArrowUp,
  Download,
  FlaskConical,
  ListChecks,
  Pencil,
  Plus,
  Trash2,
  Upload,
  X,
} from "lucide-react";
import { api, errorMessage } from "@/lib/api";
import { formatBytes, timeAgo, winPath } from "@/lib/format";
import { useData } from "@/hooks/useData";
import { useSettings } from "@/stores/app";
import type {
  Condition,
  FileKind,
  PresetInfo,
  Rule,
  RuleAction,
  RuleDraft,
  RuleTestMatch,
} from "@/types/api";
import { KIND_LABELS } from "@/lib/kinds";
import { PageBody, PageHeader } from "@/components/layout/PageHeader";
import { Button } from "@/components/ui/button";
import {
  Badge,
  Card,
  Checkbox,
  EmptyState,
  Input,
  Select,
  Switch,
  Tooltip,
} from "@/components/ui/primitives";
import { ConfirmDialog, Dialog } from "@/components/ui/dialog";
import { toast } from "@/components/ui/toast";
import { cn } from "@/lib/cn";

const CONDITION_TYPES: { type: Condition["type"]; label: string }[] = [
  { type: "extension_is", label: "Extension is" },
  { type: "name_contains", label: "Filename contains" },
  { type: "name_starts_with", label: "Filename starts with" },
  { type: "name_ends_with", label: "Filename ends with" },
  { type: "name_matches", label: "Filename matches pattern" },
  { type: "size_greater_than", label: "Size greater than" },
  { type: "size_less_than", label: "Size less than" },
  { type: "modified_within_days", label: "Modified within the last" },
  { type: "modified_older_than_days", label: "Modified more than … ago" },
  { type: "created_within_days", label: "Created within the last" },
  { type: "kind_is", label: "File type is" },
];

function newCondition(type: Condition["type"]): Condition {
  switch (type) {
    case "extension_is":
      return { type, values: [] };
    case "name_matches":
      return { type, pattern: "" };
    case "size_greater_than":
    case "size_less_than":
      return { type, bytes: 100 * 1024 * 1024 };
    case "modified_within_days":
    case "modified_older_than_days":
    case "created_within_days":
      return { type, days: 7 };
    case "kind_is":
      return { type, kind: "pdf" };
    default:
      return { type, value: "" } as Condition;
  }
}

export function describeCondition(c: Condition): string {
  switch (c.type) {
    case "extension_is":
      return `extension is ${c.values.map((v) => "." + v.replace(/^\./, "")).join(", ") || "…"}`;
    case "name_contains":
      return `name contains “${c.value}”`;
    case "name_starts_with":
      return `name starts with “${c.value}”`;
    case "name_ends_with":
      return `name ends with “${c.value}”`;
    case "name_matches":
      return `name matches ${c.pattern}`;
    case "size_greater_than":
      return `larger than ${formatBytes(c.bytes)}`;
    case "size_less_than":
      return `smaller than ${formatBytes(c.bytes)}`;
    case "modified_within_days":
      return `modified in the last ${c.days} days`;
    case "modified_older_than_days":
      return `modified more than ${c.days} days ago`;
    case "created_within_days":
      return `created in the last ${c.days} days`;
    case "kind_is":
      return `is a ${KIND_LABELS[c.kind].toLowerCase()}`;
  }
}

function describeAction(a: RuleAction): string {
  if (a.type === "move_to")
    return `Move to ${winPath(a.folder)}${a.confirm ? " (after confirmation)" : ""}`;
  if (a.type === "ignore") return "Leave in place";
  return "Flag for review";
}

const SIZE_UNITS = [
  { label: "KB", mult: 1024 },
  { label: "MB", mult: 1024 ** 2 },
  { label: "GB", mult: 1024 ** 3 },
];

function ConditionEditor({
  c,
  onChange,
  onRemove,
}: {
  c: Condition;
  onChange: (c: Condition) => void;
  onRemove: () => void;
}) {
  const unit =
    "bytes" in c
      ? (SIZE_UNITS.slice()
          .reverse()
          .find((u) => c.bytes % u.mult === 0 && c.bytes >= u.mult) ?? SIZE_UNITS[0])
      : SIZE_UNITS[1];
  return (
    <div className="flex items-center gap-2">
      <Select
        value={c.type}
        onChange={(e) => onChange(newCondition(e.target.value as Condition["type"]))}
        aria-label="Condition type"
        className="w-56"
      >
        {CONDITION_TYPES.map((t) => (
          <option key={t.type} value={t.type}>
            {t.label}
          </option>
        ))}
      </Select>
      {c.type === "extension_is" && (
        <Input
          defaultValue={c.values.join(", ")}
          placeholder="pdf, docx"
          aria-label="Extensions"
          onChange={(e) =>
            onChange({
              ...c,
              values: e.target.value
                .split(/[,\s]+/)
                .map((v) => v.trim())
                .filter(Boolean),
            })
          }
        />
      )}
      {(c.type === "name_contains" ||
        c.type === "name_starts_with" ||
        c.type === "name_ends_with") && (
        <Input
          value={c.value}
          placeholder="invoice"
          aria-label="Text"
          onChange={(e) => onChange({ ...c, value: e.target.value })}
        />
      )}
      {c.type === "name_matches" && (
        <Input
          value={c.pattern}
          placeholder="Invoice-*.pdf"
          aria-label="Wildcard pattern"
          onChange={(e) => onChange({ ...c, pattern: e.target.value })}
        />
      )}
      {(c.type === "size_greater_than" || c.type === "size_less_than") && (
        <>
          <Input
            type="number"
            min={0}
            className="w-28"
            aria-label="Size"
            value={Math.round(c.bytes / unit.mult)}
            onChange={(e) =>
              onChange({ ...c, bytes: Math.max(0, Number(e.target.value)) * unit.mult })
            }
          />
          <Select
            aria-label="Unit"
            value={unit.label}
            onChange={(e) =>
              onChange({
                ...c,
                bytes:
                  Math.round(c.bytes / unit.mult) *
                  SIZE_UNITS.find((u) => u.label === e.target.value)!.mult,
              })
            }
          >
            {SIZE_UNITS.map((u) => (
              <option key={u.label}>{u.label}</option>
            ))}
          </Select>
        </>
      )}
      {(c.type === "modified_within_days" ||
        c.type === "modified_older_than_days" ||
        c.type === "created_within_days") && (
        <>
          <Input
            type="number"
            min={0}
            max={36500}
            className="w-24"
            aria-label="Days"
            value={c.days}
            onChange={(e) => onChange({ ...c, days: Math.max(0, Number(e.target.value)) })}
          />
          <span className="text-[13px] text-muted">days</span>
        </>
      )}
      {c.type === "kind_is" && (
        <Select
          aria-label="File type"
          value={c.kind}
          onChange={(e) => onChange({ ...c, kind: e.target.value as FileKind })}
        >
          {(Object.keys(KIND_LABELS) as FileKind[]).map((k) => (
            <option key={k} value={k}>
              {KIND_LABELS[k]}
            </option>
          ))}
        </Select>
      )}
      <Button variant="ghost" size="icon" onClick={onRemove} aria-label="Remove condition">
        <X className="size-4" />
      </Button>
    </div>
  );
}

const EMPTY: RuleDraft = {
  name: "",
  description: "",
  enabled: true,
  matchMode: "all",
  conditions: [{ type: "extension_is", values: [] }],
  action: { type: "move_to", folder: "", confirm: false },
};

function RuleEditor({
  rule,
  open,
  onClose,
  onSaved,
}: {
  rule: Rule | null;
  open: boolean;
  onClose: () => void;
  onSaved: () => void;
}) {
  const [d, setD] = useState<RuleDraft>(EMPTY);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [test, setTest] = useState<RuleTestMatch[] | null>(null);
  const [presets, setPresets] = useState<PresetInfo[]>([]);
  const preset = useSettings()?.preset ?? "balanced";

  useEffect(() => {
    if (open) {
      setD(
        rule
          ? {
              name: rule.name,
              description: rule.description,
              enabled: rule.enabled,
              matchMode: rule.matchMode,
              conditions: rule.conditions,
              action: rule.action,
            }
          : EMPTY,
      );
      setErr(null);
      setTest(null);
      void api.presets().then(setPresets);
    }
  }, [open, rule]);

  const folders = presets.find((p) => p.id === preset)?.folders ?? [];

  const save = async () => {
    setBusy(true);
    setErr(null);
    try {
      if (rule) await api.updateRule(rule.id, d);
      else await api.createRule(d);
      toast.success(rule ? "Rule updated" : "Rule created");
      onSaved();
      onClose();
    } catch (e) {
      setErr(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const runTest = async () => {
    setErr(null);
    try {
      setTest(await api.testRule(d, rule?.id));
    } catch (e) {
      setErr(errorMessage(e));
    }
  };

  return (
    <Dialog
      open={open}
      onOpenChange={(v) => !v && onClose()}
      title={rule ? "Edit rule" : "New rule"}
      description="Rules run before the default extension-based sorting, in the order shown on the Rules screen."
      wide
      footer={
        <>
          <Button variant="ghost" onClick={runTest} className="mr-auto">
            <FlaskConical className="size-4" /> Test against my folder
          </Button>
          <Button variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button variant="primary" onClick={save} loading={busy}>
            Save rule
          </Button>
        </>
      }
    >
      <div className="space-y-4">
        <div className="grid grid-cols-2 gap-3">
          <div>
            <label className="text-[13px] font-medium" htmlFor="rule-name">
              Name
            </label>
            <Input
              id="rule-name"
              value={d.name}
              onChange={(e) => setD({ ...d, name: e.target.value })}
              placeholder="Invoices"
              className="mt-1"
            />
          </div>
          <div>
            <label className="text-[13px] font-medium" htmlFor="rule-desc">
              Description
            </label>
            <Input
              id="rule-desc"
              value={d.description}
              onChange={(e) => setD({ ...d, description: e.target.value })}
              placeholder="Optional"
              className="mt-1"
            />
          </div>
        </div>

        <div>
          <div className="mb-2 flex items-center gap-2 text-[13px] font-medium">
            When
            <Select
              aria-label="Match mode"
              value={d.matchMode}
              onChange={(e) => setD({ ...d, matchMode: e.target.value as "all" | "any" })}
              className="h-8"
            >
              <option value="all">all conditions match</option>
              <option value="any">any condition matches</option>
            </Select>
          </div>
          <div className="space-y-2">
            {d.conditions.map((c, i) => (
              <ConditionEditor
                key={i}
                c={c}
                onChange={(nc) =>
                  setD({ ...d, conditions: d.conditions.map((x, j) => (j === i ? nc : x)) })
                }
                onRemove={() => setD({ ...d, conditions: d.conditions.filter((_, j) => j !== i) })}
              />
            ))}
          </div>
          <Button
            size="sm"
            variant="ghost"
            className="mt-2"
            onClick={() =>
              setD({ ...d, conditions: [...d.conditions, newCondition("name_contains")] })
            }
            disabled={d.conditions.length >= 20}
          >
            <Plus className="size-3.5" /> Add condition
          </Button>
        </div>

        <div>
          <div className="mb-2 text-[13px] font-medium">Then</div>
          <div className="flex flex-wrap items-center gap-2">
            <Select
              aria-label="Action"
              value={d.action.type}
              onChange={(e) => {
                const t = e.target.value as RuleAction["type"];
                setD({
                  ...d,
                  action: t === "move_to" ? { type: t, folder: "", confirm: false } : { type: t },
                });
              }}
            >
              <option value="move_to">Move to folder</option>
              <option value="ignore">Leave it in place</option>
              <option value="review">Flag for later review</option>
            </Select>
            {d.action.type === "move_to" && (
              <>
                <Input
                  list="tl-folders"
                  className="w-72"
                  aria-label="Destination folder"
                  value={d.action.folder}
                  placeholder="Documents/Invoices"
                  onChange={(e) =>
                    setD({
                      ...d,
                      action: {
                        ...(d.action as Extract<RuleAction, { type: "move_to" }>),
                        folder: e.target.value,
                      },
                    })
                  }
                />
                <datalist id="tl-folders">
                  {folders.map((f) => (
                    <option key={f} value={f} />
                  ))}
                </datalist>
                <label className="flex items-center gap-2 text-[13px]">
                  <Checkbox
                    checked={d.action.confirm}
                    onCheckedChange={(v) =>
                      setD({
                        ...d,
                        action: {
                          ...(d.action as Extract<RuleAction, { type: "move_to" }>),
                          confirm: v,
                        },
                      })
                    }
                    label="Require confirmation"
                  />
                  Require confirmation
                </label>
              </>
            )}
          </div>
          {d.action.type === "move_to" && (
            <p className="mt-1.5 text-[12px] text-subtle">
              A folder inside your authorized folder. Paths outside it are rejected.
            </p>
          )}
        </div>

        {err && (
          <p className="rounded-lg bg-danger-soft px-3 py-2 text-[13px] text-danger">{err}</p>
        )}

        {test && (
          <div className="rounded-xl border border-border">
            <div className="border-b border-border px-3 py-2 text-[13px] font-semibold">
              {test.length === 0
                ? "No files currently match"
                : `${test.length} file${test.length === 1 ? "" : "s"} match`}
            </div>
            <ul className="max-h-52 divide-y divide-border overflow-auto">
              {test.map((m) => (
                <li key={m.rel} className="flex items-center gap-3 px-3 py-1.5 text-[12.5px]">
                  <span className="min-w-0 flex-1 truncate">{m.name}</span>
                  {m.decides ? (
                    <Badge tone="success">This rule decides</Badge>
                  ) : (
                    <Tooltip content={`Decided by: ${m.decidedBy ?? "another rule"}`}>
                      <span>
                        <Badge tone="warning">Overridden</Badge>
                      </span>
                    </Tooltip>
                  )}
                  <span className="w-48 truncate font-mono text-muted">
                    {m.destination ? winPath(m.destination) : "stays"}
                  </span>
                </li>
              ))}
            </ul>
          </div>
        )}
      </div>
    </Dialog>
  );
}

export function Rules() {
  const { data: rules, reload, setData } = useData(api.rules);
  const [editing, setEditing] = useState<Rule | null>(null);
  const [editorOpen, setEditorOpen] = useState(false);
  const [deleting, setDeleting] = useState<Rule | null>(null);
  const [importOpen, setImportOpen] = useState(false);
  const [importText, setImportText] = useState("");

  const move = async (i: number, dir: -1 | 1) => {
    if (!rules) return;
    const ids = rules.map((r) => r.id);
    const j = i + dir;
    [ids[i], ids[j]] = [ids[j], ids[i]];
    try {
      setData(await api.reorderRules(ids));
    } catch (e) {
      toast.error("Could not reorder", errorMessage(e));
    }
  };

  const toggle = async (r: Rule, enabled: boolean) => {
    try {
      await api.updateRule(r.id, {
        name: r.name,
        description: r.description,
        enabled,
        matchMode: r.matchMode,
        conditions: r.conditions,
        action: r.action,
      });
      void reload();
    } catch (e) {
      toast.error("Could not update rule", errorMessage(e));
    }
  };

  const exportRules = async () => {
    try {
      const json = await api.exportRules();
      await navigator.clipboard.writeText(json);
      toast.success(
        "Rules copied to the clipboard",
        "Paste them into a file or into Import on another PC.",
      );
    } catch (e) {
      toast.error("Export failed", errorMessage(e));
    }
  };

  const doImport = async () => {
    try {
      const n = await api.importRules(importText);
      toast.success(`${n} rule${n === 1 ? "" : "s"} imported`);
      setImportOpen(false);
      setImportText("");
      void reload();
    } catch (e) {
      toast.error("Import failed", errorMessage(e));
    }
  };

  return (
    <>
      <PageHeader
        title="Rules"
        subtitle="Your rules run first, top to bottom. The first matching rule decides; the default style handles everything else."
        actions={
          <>
            <Button onClick={() => setImportOpen(true)}>
              <Upload className="size-4" /> Import
            </Button>
            <Button onClick={exportRules} disabled={!rules?.length}>
              <Download className="size-4" /> Export
            </Button>
            <Button
              variant="primary"
              onClick={() => {
                setEditing(null);
                setEditorOpen(true);
              }}
            >
              <Plus className="size-4" /> New rule
            </Button>
          </>
        }
      />
      <PageBody>
        <Card>
          {rules && rules.length === 0 && (
            <EmptyState
              icon={<ListChecks className="size-5" />}
              title="No custom rules yet"
              action={
                <Button
                  variant="primary"
                  onClick={() => {
                    setEditing(null);
                    setEditorOpen(true);
                  }}
                >
                  <Plus className="size-4" /> Create your first rule
                </Button>
              }
            >
              For example: move PDFs whose name contains “invoice” into Documents › Invoices, or
              leave very large files alone.
            </EmptyState>
          )}
          <ul className="divide-y divide-border">
            {rules?.map((r, i) => (
              <li
                key={r.id}
                className={cn("flex items-center gap-4 px-5 py-3.5", !r.enabled && "opacity-60")}
              >
                <span className="tl-num w-6 text-center text-[13px] font-semibold text-subtle">
                  {i + 1}
                </span>
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2">
                    <span className="truncate text-[14px] font-semibold">{r.name}</span>
                    {!r.enabled && <Badge>Disabled</Badge>}
                  </div>
                  <p className="mt-0.5 truncate text-[12.5px] text-muted">
                    If{" "}
                    {r.conditions
                      .map(describeCondition)
                      .join(r.matchMode === "all" ? " and " : " or ")}{" "}
                    → {describeAction(r.action)}
                  </p>
                  {r.description && (
                    <p className="truncate text-[12px] text-subtle">{r.description}</p>
                  )}
                </div>
                <div className="hidden w-36 text-right text-[12px] text-subtle md:block">
                  <div>
                    {r.matchCount} file{r.matchCount === 1 ? "" : "s"} moved
                  </div>
                  <div>
                    {r.lastTriggeredAt ? `Last used ${timeAgo(r.lastTriggeredAt)}` : "Not used yet"}
                  </div>
                </div>
                <Switch
                  checked={r.enabled}
                  onCheckedChange={(v) => void toggle(r, v)}
                  label={`Enable ${r.name}`}
                />
                <div className="flex">
                  <Button
                    variant="ghost"
                    size="icon"
                    aria-label="Move up"
                    disabled={i === 0}
                    onClick={() => move(i, -1)}
                  >
                    <ArrowUp className="size-4" />
                  </Button>
                  <Button
                    variant="ghost"
                    size="icon"
                    aria-label="Move down"
                    disabled={i === rules.length - 1}
                    onClick={() => move(i, 1)}
                  >
                    <ArrowDown className="size-4" />
                  </Button>
                  <Button
                    variant="ghost"
                    size="icon"
                    aria-label={`Edit ${r.name}`}
                    onClick={() => {
                      setEditing(r);
                      setEditorOpen(true);
                    }}
                  >
                    <Pencil className="size-4" />
                  </Button>
                  <Button
                    variant="ghost"
                    size="icon"
                    aria-label={`Delete ${r.name}`}
                    onClick={() => setDeleting(r)}
                  >
                    <Trash2 className="size-4" />
                  </Button>
                </div>
              </li>
            ))}
          </ul>
        </Card>
      </PageBody>
      <RuleEditor
        rule={editing}
        open={editorOpen}
        onClose={() => setEditorOpen(false)}
        onSaved={() => void reload()}
      />
      <ConfirmDialog
        open={!!deleting}
        onOpenChange={(v) => !v && setDeleting(null)}
        title={`Delete “${deleting?.name}”?`}
        description="Files it already moved stay where they are and can still be undone from Activity."
        confirmLabel="Delete rule"
        danger
        onConfirm={async () => {
          try {
            await api.deleteRule(deleting!.id);
            setDeleting(null);
            void reload();
          } catch (e) {
            toast.error("Could not delete", errorMessage(e));
          }
        }}
      />
      <Dialog
        open={importOpen}
        onOpenChange={setImportOpen}
        title="Import rules"
        description="Paste a Tucklane rules export. Every rule is validated; destinations outside your folder are rejected."
        footer={
          <>
            <Button variant="ghost" onClick={() => setImportOpen(false)}>
              Cancel
            </Button>
            <Button variant="primary" onClick={doImport} disabled={!importText.trim()}>
              Import
            </Button>
          </>
        }
      >
        <textarea
          value={importText}
          onChange={(e) => setImportText(e.target.value)}
          className="h-56 w-full rounded-lg border border-border-strong bg-surface p-3 font-mono text-[12px]"
          aria-label="Rules JSON"
          placeholder='{"format":"tucklane-rules","version":1,"rules":[…]}'
        />
      </Dialog>
    </>
  );
}
