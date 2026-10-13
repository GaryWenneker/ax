import {Children, isValidElement, useMemo, type ReactNode, type ReactElement} from 'react';
import {ColumnHeader, ColumnTools, ItemColumnContext, columnFromHeader, columnText, useListColumns, type ListColumn} from './ListColumns';
import type {ColumnValue} from '../../lib/columnModel';

export function ItemList({ children, className = '', columns: suppliedColumns, listId = '' }: { children: ReactNode; className?: string; columns?: readonly ListColumn[]; listId?: string }) {
  const trace = /mcp-trace-calm|mcp-blade-steps/.test(className);
  // Repository-owned rows expose presentation props or explicit raw listValues for typed sorting.
  const parts = Children.toArray(children);
  const legacyHeader = parts.find(child => isValidElement<{className?: string}>(child) && child.props.className?.includes('portal-data-header')) as ReactElement<{children?: ReactNode}> | undefined;
  const columns = suppliedColumns ?? (legacyHeader ? Children.toArray(legacyHeader.props.children).map(columnFromHeader) : trace ? [{label:'Event'}, {label:'Time'}, {label:'Context'}] : [{label:'Name'}, {label:'Details'}, {label:'Labels'}]);
  const controls = useListColumns(columns, listId || (trace ? 'trace' : 'items'));
  const records = parts.filter(child => child !== legacyHeader).filter(isValidElement).map(child => {
    const props = child.props as {listValues?: readonly ColumnValue[]; title?: string; subtitle?: string; meta?: string; metaTitle?: string; badges?: ReactNode; portalColumns?: ReactNode[]};
    return {element: child, values: props.listValues ?? (props.portalColumns ? [props.title, ...props.portalColumns.map(columnText), columnText(props.badges)] : trace ? [props.title, props.metaTitle ?? props.meta, columnText(props.badges)] : [props.title, props.subtitle, columnText(props.badges)])};
  });
  const display = controls.order(records);
  const labelsKey = columns.map(c => c.label).join("|");
  const context = useMemo(() => ({trace, labels: labelsKey.split("|")}), [trace, labelsKey]);
  return <ItemColumnContext.Provider value={context}>
    <div className="column-list-container"><ColumnTools controls={controls} />
    <div className={`page-item-list column-list${className ? ` ${className}` : ''}${controls.widths ? ' column-list--resized' : ''}${controls.sort || Object.values(controls.filters).some(v => v.trim()) ? ' column-list--custom-order' : ''}`} style={controls.style}>
      <ColumnHeader controls={controls} className={legacyHeader ? 'portal-data-header' : 'item-column-header'} />
      {display.map(row => row.element)}
      {records.length > 0 && display.length === 0 && <p className="column-empty" role="status">No loaded rows match the column filters.</p>}
    </div></div>
  </ItemColumnContext.Provider>;
}
