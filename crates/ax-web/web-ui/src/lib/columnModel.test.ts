import {test} from 'node:test';
import assert from 'node:assert/strict';
import {orderRows,validWidths,clampWidth,arrangeTree,countTreeRows,type TreeEntry} from './columnModel.ts';
const rows=[{id:'a',values:['Zulu',10]},{id:'b',values:['Alpha',2]},{id:'c',values:['Alpha',10]}];
test('sorting is stable, numeric and preserves original data',()=>{
 assert.deepEqual(orderRows(rows,{column:1,direction:'asc'},{}).map(r=>r.id),['b','a','c']);
 assert.deepEqual(orderRows(rows,{column:0,direction:'desc'},{}).map(r=>r.id),['a','b','c']);
 assert.deepEqual(orderRows(rows,null,{}),rows);assert.deepEqual(rows.map(r=>r.id),['a','b','c']);
});
test('filters intersect and case-fold without treating user text as regex',()=>{
 assert.deepEqual(orderRows(rows,null,{0:'ALP',1:'10'}).map(r=>r.id),['c']);
 assert.equal(orderRows(rows,null,{0:'['}).length,0);
});
test('formatted numeric cells and ISO dates sort by value',()=>{
 assert.deepEqual(orderRows([{values:['1,200']},{values:['20']}],{column:0,direction:'asc'},{}).map(r=>r.values[0]),['20','1,200']);
 assert.deepEqual(orderRows([{values:['2026-10-12']},{values:['2025-01-01']}],{column:0,direction:'asc'},{}).map(r=>r.values[0]),['2025-01-01','2026-10-12']);
});
test('persisted widths reject invalid schemas and stay bounded',()=>{
 assert.equal(validWidths([100,NaN],2),null);assert.equal(validWidths([100],2),null);assert.equal(validWidths('bad',2),null);
 assert.deepEqual(validWidths([100,200],2),[100,200]);assert.equal(validWidths([1,200],2),null);
 assert.equal(clampWidth(-20),64);assert.equal(clampWidth(5000),1600);
});

test('currency and percentages use decimal value order',()=>{
 assert.deepEqual(orderRows([{values:['$2.9']},{values:['$2.10']}],{column:0,direction:'asc'},{}).map(r=>r.values[0]),['$2.10','$2.9']);
 assert.deepEqual(orderRows([{values:['2.9%']},{values:['2.10%']}],{column:0,direction:'asc'},{}).map(r=>r.values[0]),['2.10%','2.9%']);
 assert.deepEqual(validWidths([20,100],2,[20,64]),[20,100]);
});

test('abbreviated numeric values sort by magnitude',()=>{
 assert.deepEqual(orderRows([{values:['1M']},{values:['500K']}],{column:0,direction:'asc'},{}).map(r=>r.values[0]),['500K','1M']);
});

test('grouped rows are filtered and sorted inside their groups, and emptied groups disappear',()=>{
 type Row={id:string;values:string[]};
 const row=(id:string,time:string,context:string):TreeEntry<Row,string,string>=>({kind:'row',row:{id,values:[id,time,context]}});
 const tree:TreeEntry<Row,string,string>[]=[
  {kind:'group',group:'session-1',children:[{kind:'fixed',item:'summary-1'},{kind:'group',group:'turn-1',children:[{kind:'fixed',item:'turn-summary'},row('late','20:00:02','IN Search'),row('early','20:00:01','OUT Search'),row('status','20:00:00','INT Status')]}]},
  {kind:'group',group:'session-2',children:[{kind:'fixed',item:'summary-2'},row('other','19:00:00','INT Status')]},
 ];
 const flat=(entries:readonly TreeEntry<Row,string,string>[]):string[]=>entries.flatMap(e=>e.kind==='row'?[e.row.id]:e.kind==='group'?[e.group,...flat(e.children)]:[e.item]);
 const filtered=arrangeTree(tree,rows=>orderRows(rows,{column:1,direction:'asc'},{2:'search'}));
 assert.deepEqual(flat(filtered),['session-1','summary-1','turn-1','turn-summary','early','late']);
 assert.equal(countTreeRows(filtered),2);
 assert.deepEqual(flat(arrangeTree(tree,rows=>orderRows(rows,null,{}))),flat(tree));
 assert.equal(countTreeRows(tree),4);
 assert.equal(countTreeRows(arrangeTree(tree,rows=>orderRows(rows,null,{0:'missing'}))),0);
});
