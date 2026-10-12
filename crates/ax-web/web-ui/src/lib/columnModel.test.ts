import {test} from 'node:test';
import assert from 'node:assert/strict';
import {orderRows,validWidths,clampWidth} from './columnModel.ts';
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
