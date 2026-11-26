import assert from 'node:assert/strict';
import { test } from 'node:test';
import { argumentsFromValues, fieldsFromSchema } from './schemaForm.ts';

const SCHEMA = {
  type: 'object',
  properties: {
    limit: { type: 'number', description: 'Rows per page' },
    query: { type: 'string' },
    depth: { type: 'string', enum: ['hot', 'archive'] },
    force: { type: 'boolean' },
    files: { type: 'array', items: { type: 'string' } },
    session: { type: 'string' },
  },
  required: ['query'],
};

test('fields follow the schema, required first, without the session id', () => {
  const fields = fieldsFromSchema(SCHEMA);
  assert.deepEqual(fields.map((f) => [f.name, f.kind, f.required]), [
    ['query', 'string', true],
    ['limit', 'number', false],
    ['depth', 'enum', false],
    ['force', 'boolean', false],
    ['files', 'json', false],
  ]);
  assert.deepEqual(fields[2].options, ['hot', 'archive']);
  assert.equal(fields[1].description, 'Rows per page');
});

test('a missing or odd schema gives no fields', () => {
  for (const schema of [undefined, null, {}, { properties: null }, 'x', { properties: 'x' }]) {
    assert.deepEqual(fieldsFromSchema(schema), []);
  }
  assert.deepEqual(fieldsFromSchema({ properties: { a: null, b: 3 } }).map((f) => f.kind), ['json', 'json']);
});

test('values become typed arguments and empty optional fields are left out', () => {
  const fields = fieldsFromSchema(SCHEMA);
  assert.deepEqual(
    argumentsFromValues(fields, { query: '  parser  ', limit: '25', depth: 'archive', force: true, files: '["a.rs"]' }),
    { query: 'parser', limit: 25, depth: 'archive', force: true, files: ['a.rs'] },
  );
  assert.deepEqual(argumentsFromValues(fields, { query: 'x', limit: ' ', force: false }), { query: 'x' });
});

test('unusable values name their field', () => {
  const fields = fieldsFromSchema(SCHEMA);
  assert.throws(() => argumentsFromValues(fields, {}), /query is required/);
  assert.throws(() => argumentsFromValues(fields, { query: 'x', limit: 'ten' }), /limit must be a number/);
  assert.throws(() => argumentsFromValues(fields, { query: 'x', files: '[a' }), /files must be valid JSON/);
});

test('a required boolean is always sent', () => {
  const fields = fieldsFromSchema({ properties: { force: { type: 'boolean' } }, required: ['force'] });
  assert.deepEqual(argumentsFromValues(fields, {}), { force: false });
  assert.deepEqual(argumentsFromValues(fields, { force: true }), { force: true });
});
