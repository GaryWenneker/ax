/**
 * List state shared by the Rules and Skills pages: which row is open, the
 * multi-row selection, the closing animation of the detail blade, and the
 * collapsed groups and group filter kept in browser storage.
 */
import { useEffect, useRef, useState } from 'react';
import { POLICY_BLADE_DISMISS_MS } from './policyBladeMotion';
import { isGlobalPolicy } from '../components/ui/policyListUtils';
import { resolveOpenTarget } from './policySelection';
import { toggleCollapsed } from '../skillGroups';
import { loadJson, saveJson } from './uiStorage';

type OriginRow = { origin?: string; projectId?: number };

export function usePolicySelection<Row extends OriginRow>({
  routeKey,
  routeOrigin,
  routeProjectId,
  rows,
  keyOf,
  onSelect,
}: {
  routeKey: string | null;
  routeOrigin: string | null | undefined;
  routeProjectId: number | null | undefined;
  rows: Row[];
  keyOf: (row: Row) => string;
  onSelect: (key: string | null, origin?: string, projectId?: number) => void;
}) {
  const [selectedKey, setSelectedKey] = useState<string | null>(routeKey);
  const [openOrigin, setOpenOrigin] = useState<string | undefined>();
  const [openProjectId, setOpenProjectId] = useState<number | undefined>();
  const [selectedIds, setSelectedIds] = useState<Set<string>>(() => new Set(routeKey ? [routeKey] : []));
  const [anchorId, setAnchorId] = useState<string | null>(routeKey);
  const [bladeClosing, setBladeClosing] = useState(false);
  const onSelectRef = useRef(onSelect);
  onSelectRef.current = onSelect;

  useEffect(() => {
    setSelectedKey(routeKey);
    if (routeKey) {
      setSelectedIds(new Set([routeKey]));
      setAnchorId(routeKey);
    }
  }, [routeKey]);

  useEffect(() => {
    if (!routeKey) return;
    const t = resolveOpenTarget(
      rows.map((r) => ({ key: keyOf(r), origin: r.origin, projectId: r.projectId })),
      routeKey,
      routeOrigin,
      routeProjectId,
    );
    setOpenOrigin(t.origin);
    setOpenProjectId(t.projectId);
  }, [routeKey, routeOrigin, routeProjectId, rows]);

  function select(key: string | null, row?: OriginRow) {
    setBladeClosing(false);
    setSelectedKey(key);
    const global = row && isGlobalPolicy(row);
    onSelectRef.current(key, global ? 'global' : undefined, global ? row.projectId : undefined);
    if (row) {
      setOpenOrigin(row.origin);
      setOpenProjectId(row.projectId);
    }
    if (key) {
      setSelectedIds(new Set([key]));
      setAnchorId(key);
    } else if (selectedIds.size <= 1) {
      setSelectedIds(new Set());
    }
  }

  function requestCloseBlade() {
    if (!selectedKey || bladeClosing) return;
    setBladeClosing(true);
  }

  useEffect(() => {
    if (!bladeClosing) return;
    const t = window.setTimeout(() => {
      setBladeClosing(false);
      setSelectedKey(null);
      onSelectRef.current(null);
      setSelectedIds((prev) => (prev.size <= 1 ? new Set() : prev));
    }, POLICY_BLADE_DISMISS_MS);
    return () => window.clearTimeout(t);
  }, [bladeClosing]);

  return {
    selectedKey,
    setSelectedKey,
    openOrigin,
    setOpenOrigin,
    openProjectId,
    setOpenProjectId,
    selectedIds,
    setSelectedIds,
    anchorId,
    setAnchorId,
    bladeClosing,
    setBladeClosing,
    onSelectRef,
    select,
    requestCloseBlade,
  };
}

/** Collapsed groups and the group filter, kept under `<storagePrefix>-groups-*`. */
export function usePolicyGroups(storagePrefix: 'rules' | 'skills') {
  const collapsedKey = `${storagePrefix}-groups-collapsed`;
  const filterKey = `${storagePrefix}-groups-filter`;
  const [collapsed, setCollapsed] = useState<Set<string>>(() => new Set(loadJson<string[]>(collapsedKey, [])));
  const [groupIds, setGroupIds] = useState<string[]>(() => loadJson<string[]>(filterKey, []));

  function persistCollapsed(next: Set<string>) {
    saveJson(collapsedKey, [...next]);
    setCollapsed(next);
  }

  function persistGroupIds(next: string[]) {
    saveJson(filterKey, next);
    setGroupIds(next);
  }

  function toggleGroup(id: string) {
    persistCollapsed(toggleCollapsed(collapsed, id));
  }

  return { collapsed, groupIds, persistCollapsed, persistGroupIds, toggleGroup };
}
