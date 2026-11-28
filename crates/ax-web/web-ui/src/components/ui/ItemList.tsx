import {Children, cloneElement, isValidElement, useMemo, useRef, type ReactNode, type ReactElement} from 'react';
import {ColumnHeader, ColumnTools, ItemColumnContext, columnFromHeader, columnText, useListColumns, type ListColumn} from './ListColumns';
import {arrangeTree, countTreeRows, type ColumnValue, type TreeEntry} from '../../lib/columnModel';
import {useColumnFit} from '../../hooks/useColumnFit';

export function ItemList({ children, className = '', columns: suppliedColumns, listId = '', rows = false }: { children: ReactNode; className?: string; columns?: readonly ListColumn[]; listId?: string; rows?: boolean }) {
  const trace = /mcp-trace-calm|mcp-blade-steps/.test(className);
  // Repository-owned rows expose presentation props or explicit raw listValues for typed sorting.
  const parts = Children.toArray(children);
  const legacyHeader = parts.find(child => isValidElement<{className?: string}>(child) && child.props.className?.includes('portal-data-header')) as ReactElement<{children?: ReactNode}> | undefined;
  const columns = suppliedColumns ?? (legacyHeader ? Children.toArray(legacyHeader.props.children).map(columnFromHeader) : trace ? [{label:'Event'}, {label:'Time'}, {label:'Context'}] : [{label:'Name'}, {label:'Details'}, {label:'Labels'}]);
  const controls = useListColumns(columns, listId || (trace ? 'trace' : 'items'));
  const tree = toTree(parts.filter(child => child !== legacyHeader).filter(isValidElement), trace, true);
  const arranged = arrangeTree(tree, controls.order);
  const rowCount = countTreeRows(tree);
  const shownCount = countTreeRows(arranged);
  const labelsKey = columns.map(c => c.label).join("|");
  const context = useMemo(() => ({trace, labels: labelsKey.split("|")}), [trace, labelsKey]);
  const list = useRef<HTMLDivElement>(null);
  // Without manual widths the columns fit their content client-side and fill the list exactly.
  const auto = useColumnFit(list, controls.key, columns.length, controls.minimums, controls.widths !== null, `${rowCount}:${shownCount}`);
  const fitted = controls.widths === null && auto.fit !== null;
  if (rows) return <div className={`page-item-list${className ? ` ${className}` : ''}`}>{children}</div>;
  return <ItemColumnContext.Provider value={context}>
    <div className="column-list-container"><ColumnTools controls={controls} />
    <div ref={list} className={`page-item-list column-list${className ? ` ${className}` : ''}${controls.widths ? ' column-list--resized' : ''}${fitted ? ' column-list--fitted' : ''}${controls.sort || Object.values(controls.filters).some(v => v.trim()) ? ' column-list--custom-order' : ''}`} style={controls.style ?? auto.style}>
      <ColumnHeader controls={controls} className={legacyHeader ? 'portal-data-header' : 'item-column-header'} />
      {fromTree(arranged)}
      {rowCount > 0 && shownCount === 0 && <p className="column-empty" role="status">No loaded rows match the column filters.</p>}
    </div></div>
  </ItemColumnContext.Provider>;
}

type RowProps = {listValues?: readonly ColumnValue[]; title?: string; subtitle?: string; meta?: string; metaTitle?: string; badges?: ReactNode; portalColumns?: ReactNode[]; children?: ReactNode};
type ListRecord = {element: ReactElement; values: readonly ColumnValue[]};
type ListEntry = TreeEntry<ListRecord, ReactElement<RowProps>, ReactElement>;

const isRow = (props: RowProps) => props.listValues !== undefined || props.title !== undefined || props.portalColumns !== undefined;

function rowValues(props: RowProps, trace: boolean): readonly ColumnValue[] {
  return props.listValues ?? (props.portalColumns ? [props.title, ...props.portalColumns.map(columnText), columnText(props.badges)] : trace ? [props.title, props.metaTitle ?? props.meta, columnText(props.badges)] : [props.title, props.subtitle, columnText(props.badges)]);
}

/**
 * Splits list children into rows, groups that hold rows (Logging's session and turn sections), and fixed
 * parts such as group headings, so sorting and filtering reach rows inside groups. Top-level elements
 * that are neither a row nor hold rows count as rows, as they always have.
 */
function toTree(children: readonly ReactNode[], trace: boolean, top: boolean): ListEntry[] {
  return children.filter(isValidElement).map((child): ListEntry => {
    const element = child as ReactElement<RowProps>;
    const props = element.props;
    if (!isRow(props)) {
      const nested = toTree(Children.toArray(props.children), trace, false);
      if (countTreeRows(nested) > 0) return {kind: 'group', group: element, children: nested};
      if (!top) return {kind: 'fixed', item: element};
    }
    return {kind: 'row', row: {element, values: rowValues(props, trace)}};
  });
}

function fromTree(entries: readonly ListEntry[]): ReactNode[] {
  return entries.map(entry => entry.kind === 'row' ? entry.row.element : entry.kind === 'fixed' ? entry.item : cloneElement(entry.group, undefined, ...fromTree(entry.children)));
}
