import {Children, cloneElement, isValidElement, type CSSProperties, type ReactNode, type ReactElement, type AriaAttributes} from 'react';
import {ColumnHeading, ColumnTools, columnText, useListColumns} from './ListColumns';

export function DataTable({ children, dense, className = '' }: { children: ReactNode; dense?: boolean; className?: string }) {
  // The sections/cells below are repository-owned JSX, retaining their original callbacks and props.
  const parts = Children.toArray(children).filter(isValidElement) as ReactElement<{children?: ReactNode}>[];
  const head = parts.find(part => part.type === 'thead');
  const body = parts.find(part => part.type === 'tbody');
  const headRow = head && Children.toArray(head.props.children).find(isValidElement) as ReactElement<{children?: ReactNode}> | undefined;
  const cells = Children.toArray(headRow?.props.children).filter(isValidElement) as ReactElement<{children?: ReactNode; style?: CSSProperties; 'aria-sort'?: AriaAttributes['aria-sort']}>[];
  const columns = cells.map((cell, i) => ({label: columnText(cell.props.children) || `Column ${i + 1}`, sortable: !/action|distribution/i.test(columnText(cell.props.children)), filterable: !/action|distribution/i.test(columnText(cell.props.children))}));
  const controls = useListColumns(columns, 'table');
  const rows = Children.toArray(body?.props.children).filter(isValidElement) as ReactElement<{children?: ReactNode}>[];
  const records = rows.map(row => ({element: row, values: Children.toArray(row.props.children).map(columnText)}));
  const display = controls.order(records);
  return (
    <div className="column-table-container">
      {columns.length > 0 && <ColumnTools controls={controls} />}
      <div className="page-table-wrap">
      <table className={`page-table column-table${dense ? ' page-table--dense policy-table' : ''} ${className}`} style={controls.widths ? {tableLayout: 'fixed', width: `${controls.widths.reduce((sum, width) => sum + width, 0)}px`} : undefined}>
        {controls.widths && <colgroup>{controls.widths.map((width, i) => <col key={i} style={{width}} />)}</colgroup>}
        {parts.map(part => part === head && headRow ? cloneElement(part, {}, cloneElement(headRow, {}, cells.map((cell, index) => cloneElement(cell, {key: index, 'aria-sort':controls.sort?.column === index ? controls.sort.direction === 'asc' ? 'ascending' : 'descending' : 'none', style: controls.widths ? undefined : cell.props.style}, <ColumnHeading controls={controls} index={index} />)))) : part === body ? cloneElement(part, {}, display.map(row => row.element)) : part)}
      </table>
      </div>
      {records.length > 0 && display.length === 0 && <p className="column-empty" role="status">No loaded rows match the column filters.</p>}
    </div>
  );
}
