import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  getApplicationCatalog,
  getPreferencesSnapshot,
  refreshApplicationCatalog,
  updatePreferences,
} from "@/lib/tauri";
import { useAlertStore } from "@/stores/alertStore";
import { Input } from "@/ui/Input";
import { Button } from "@/ui/Button";
import { Spinner } from "@/ui/Spinner";

/** v1 debounced dirty computation cadence. */
const UPDATE_DEBOUNCE_MS = 150;

/**
 * PreferencesTab — port of v1 `preferencesView.js`: assistant personality
 * field and the per-application allowlist table, saved through
 * `update_preferences` (`{ payload: { personality?, allowlist: [...] } }`).
 */
export function PreferencesTab() {
  const [snapshot, setSnapshot] = useState<{ personality?: string; allowlist?: Array<{ appId: string; enabled: boolean }> } | null>(null);
  const [catalog, setCatalog] = useState<AppRow[]>([]);
  const [personality, setPersonality] = useState("");
  const [overrides, setOverrides] = useState<Map<string, boolean>>(new Map());
  const [filter, setFilter] = useState("");
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [status, setStatus] = useState<{ text: string; tone: "info" | "success" | "error" } | null>(
    null,
  );

  async function load(): Promise<void> {
    setLoading(true);
    try {
      let catalogSnapshot: { applications?: AppCatalogApp[] } | null = null;
      try {
        catalogSnapshot = await refreshApplicationCatalog();
      } catch {
        try {
          catalogSnapshot = await getApplicationCatalog();
        } catch {
          catalogSnapshot = null;
        }
      }
      const fresh = await getPreferencesSnapshot();
      applySnapshot(fresh, catalogSnapshot?.applications || []);
      setStatus({ text: "Preferences loaded", tone: "info" });
    } catch {
      setStatus({ text: "Failed to load preferences", tone: "error" });
    } finally {
      setLoading(false);
    }
  }

  function applySnapshot(
    next: { personality?: string; allowlist?: Array<{ appId: string; enabled: boolean }> } | null,
    applications: AppCatalogApp[],
  ): void {
    setSnapshot(next);
    setPersonality(next?.personality ?? "");
    const map = new Map<string, boolean>();
    (next?.allowlist ?? []).forEach((entry) => {
      if (entry?.appId) {
        map.set(entry.appId, entry.enabled !== false);
      }
    });
    setOverrides(map);
    setCatalog(
      applications.map((app) => ({
        appId: normalizeId(app),
        name: app.name ?? normalizeId(app),
      })),
    );
  }

  useEffect(() => {
    void load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Reload when the backend signals a config/preferences change. v1 parity:
  // never clobber unsaved local edits — warn instead, let the user reload.
  const preferencesTick = useAlertStore((s) => s.preferencesTick);
  const dirtyRef = useRef(false);
  useEffect(() => {
    if (preferencesTick > 0) {
      if (dirtyRef.current) {
        setStatus({
          text: "Preferences updated elsewhere — reload to see the latest values.",
          tone: "info",
        });
      } else {
        void load();
      }
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [preferencesTick]);

  // Debounced dirty computation (v1 cadence).
  const dirty = useMemo(() => {
    const initialPersonality = snapshot?.personality ?? "";
    const allowlistIds = (snapshot?.allowlist ?? []).map((e) => e.appId);
    const sameSet =
      overrides.size === allowlistIds.length &&
      allowlistIds.every((id) => overrides.has(id) && overrides.get(id) === true);
    return personality.trim() !== initialPersonality || !sameSet;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [personality, overrides, snapshot]);
  dirtyRef.current = dirty;

  const visibleRows = useMemo(() => {
    const needle = filter.trim().toLowerCase();
    const rows = catalog.filter(
      (app) => !needle || app.name.toLowerCase().includes(needle),
    );
    return rows.sort((a, b) => a.name.localeCompare(b.name));
  }, [catalog, filter]);

  // v1 "N/M apps allowed (+N disabled)" status line.
  const allowedCount = useMemo(
    () => Array.from(overrides.values()).filter(Boolean).length,
    [overrides],
  );
  const disabledCount = overrides.size - allowedCount;

  const save = useCallback(async () => {
    if (saving) {
      return;
    }
    const payload: { personality?: string; allowlist?: Array<{ appId: string; enabled: boolean }> } =
      {};
    const personalityValue = personality.trim();
    if (personalityValue !== (snapshot?.personality ?? "")) {
      payload.personality = personalityValue;
    }
    payload.allowlist = Array.from(overrides.entries()).map(([appId, enabled]) => ({
      appId,
      enabled,
    }));
    if (!payload.allowlist.length && payload.personality === undefined) {
      setStatus({ text: "No changes to save.", tone: "info" });
      return;
    }

    setSaving(true);
    setStatus({ text: "Saving preferences…", tone: "info" });
    try {
      const updated = await updatePreferences(payload);
      applySnapshot(updated, catalog.map((c) => ({ appId: c.appId, name: c.name })));
      setStatus({ text: "Preferences saved", tone: "success" });
    } catch {
      setStatus({ text: "Failed to update preferences", tone: "error" });
    } finally {
      setSaving(false);
    }
  }, [personality, overrides, saving, snapshot, catalog]);

  function toggleApp(appId: string): void {
    setOverrides((prev) => {
      const next = new Map(prev);
      next.set(appId, !(next.get(appId) ?? false));
      return next;
    });
  }

  function resetChanges(): void {
    if (!snapshot) {
      return;
    }
    applySnapshot(snapshot, catalog.map((c) => ({ appId: c.appId, name: c.name })));
    setStatus({ text: "Changes discarded", tone: "info" });
  }

  return (
    <div className="flex flex-col gap-4" data-selectable="true">
      <section>
        <h4 className="mb-1 text-[12px] font-semibold uppercase tracking-wider text-sprite-muted">
          Personality
        </h4>
        <textarea
          value={personality}
          onChange={(e) => setPersonality(e.target.value)}
          rows={3}
          maxLength={512}
          placeholder="Describe how Sprite should talk with you…"
          className="w-full rounded-md border border-sprite-border bg-sprite-bg px-2 py-1.5 text-[12.5px] text-sprite-text outline-none placeholder:text-sprite-muted focus:border-sprite-accent"
        />
        <p className="mt-0.5 text-right text-[10.5px] text-sprite-muted">
          {personality.length}/512
        </p>
      </section>

      <section>
        <h4 className="mb-1 text-[12px] font-semibold uppercase tracking-wider text-sprite-muted">
          Application allowlist
        </h4>
        <p className="mb-1.5 text-[11.5px] text-sprite-muted">
          Applications the assistant is allowed to drive on your behalf.
          {/* v1 status: "N/M apps allowed (+N disabled)". */}
          {catalog.length ? (
            <span className="ml-2">
              {allowedCount}/{catalog.length} apps allowed
              {disabledCount > 0 ? ` (+${disabledCount} disabled)` : ""}
            </span>
          ) : null}
        </p>

        <div className="flex items-center gap-2 pb-2">
          <Input
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
            placeholder="Search applications…"
            className="max-w-[240px]"
          />
          <Button onClick={() => setOverrides(new Map())} disabled={!catalog.length}>
            Reset
          </Button>
          <Button
            onClick={() =>
              setOverrides(new Map(catalog.map((app) => [app.appId, false] as const)))
            }
            disabled={!catalog.length}
          >
            None
          </Button>
          <Button
            onClick={() =>
              setOverrides(new Map(catalog.map((app) => [app.appId, true] as const)))
            }
            disabled={!catalog.length}
          >
            All
          </Button>
        </div>

        {loading ? (
          <div className="flex items-center gap-2 text-[12px] text-sprite-muted">
            <Spinner /> Loading installed applications…
          </div>
        ) : visibleRows.length === 0 ? (
          <p className="text-[12px] text-sprite-muted">
            Failed to load installed applications.
          </p>
        ) : (
          <div className="max-h-72 overflow-y-auto rounded-md border border-sprite-border">
            <table className="w-full text-[12.5px]">
              <thead className="sticky top-0 bg-sprite-surface text-left text-[11px] uppercase tracking-wider text-sprite-muted">
                <tr>
                  <th className="px-2 py-1.5">Application</th>
                  <th className="w-24 px-2 py-1.5">Allowed</th>
                </tr>
              </thead>
              <tbody>
                {visibleRows.map((app) => (
                  <tr key={app.appId} className="border-t border-sprite-border">
                    <td className="px-2 py-1.5 text-sprite-text">
                      {app.name}
                      <span className="ml-2 text-[10.5px] text-sprite-muted">{app.appId}</span>
                    </td>
                    <td className="px-2 py-1.5">
                      <input
                        type="checkbox"
                        checked={overrides.get(app.appId) ?? false}
                        onChange={() => toggleApp(app.appId)}
                      />
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </section>

      <div className="flex items-center gap-2">
        <Button variant="primary" disabled={!dirty || saving} onClick={() => void save()}>
          {saving ? <Spinner /> : "Save preferences"}
        </Button>
        <Button onClick={resetChanges} disabled={saving}>
          Discard changes
        </Button>
        {status ? (
          <span
            className={`text-[12px] ${
              status.tone === "error"
                ? "text-sprite-danger"
                : status.tone === "success"
                  ? "text-sprite-success"
                  : "text-sprite-muted"
            }`}
          >
            {status.text}
          </span>
        ) : null}
      </div>
    </div>
  );
}

interface AppCatalogApp {
  app_id?: string;
  appId?: string;
  identifier?: string;
  name?: string;
}

interface AppRow {
  appId: string;
  name: string;
}

/** v1 `normalizeId`: app_id || appId || identifier || name. */
function normalizeId(app: AppCatalogApp): string {
  return app.app_id || app.appId || app.identifier || app.name || "";
}

/** Unused at the moment, kept to mirror the v1 debounce cadence. */
export { UPDATE_DEBOUNCE_MS };
