import assert from 'node:assert/strict';
import { describe, it } from 'node:test';
import { centerDiffPopup, commitsForMemory, diffLineClass, fileChangeClass, gitFileLinkProps, memoryImageCount, memoryImageUrl, placeDiffPopup, shouldDismissDiffPopup, splitMemoryBody, zoomAt } from './memoryDetail.ts';

describe('splitMemoryBody', () => {
  it('splits a turn body into prompt, files, commits, and outcome', () => {
    const body = [
      'als je op het azure devops board kijkt, zie je dat ik meer tickets open heb staan.',
      '',
      'Files: .husky/commit-msg, package.json',
      '',
      'Commits:',
      '- 9183e498c Keep only the commit-msg hook.',
      '',
      'Outcome: Nine open pull requests now match PR 421.',
    ].join('\n');
    assert.deepEqual(splitMemoryBody(body), {
      prompt: 'als je op het azure devops board kijkt, zie je dat ik meer tickets open heb staan.',
      filesNote: '.husky/commit-msg, package.json',
      commits: '- 9183e498c Keep only the commit-msg hook.',
      changes: null,
      request: null,
      conversation: null,
      outcome: 'Nine open pull requests now match PR 421.',
    });
  });

  it('lifts request and conversation, and keeps the whole outcome as markdown', () => {
    const body = [
      'indien afbeeldingen inline moeten werken',
      '',
      'Request: 5e9a196b-902e-4ebe-893a-0ba5d47f8fef',
      '',
      'Conversation: a42e06ca4a33',
      '',
      'Outcome: ## Done',
      '',
      '- the dialog image is inline',
      '',
      '![shot](/var/folders/page.png)',
    ].join('\n');
    assert.deepEqual(splitMemoryBody(body), {
      prompt: 'indien afbeeldingen inline moeten werken',
      filesNote: null,
      commits: null,
      changes: null,
      request: '5e9a196b-902e-4ebe-893a-0ba5d47f8fef',
      conversation: 'a42e06ca4a33',
      outcome: '## Done\n\n- the dialog image is inline\n\n![shot](/var/folders/page.png)',
    });
  });

  it('keeps a Changes section out of the prompt', () => {
    const parts = splitMemoryBody('prompt\n\nChanges:\nM a.rs\nD b.rs\n\nOutcome: done');
    assert.equal(parts.prompt, 'prompt');
    assert.equal(parts.changes, 'M a.rs\nD b.rs');
    assert.equal(parts.outcome, 'done');
  });

  it('maps file changes to color classes', () => {
    assert.equal(fileChangeClass('added'), 'memory-file--added');
    assert.equal(fileChangeClass('modified'), 'memory-file--modified');
    assert.equal(fileChangeClass('deleted'), 'memory-file--deleted');
    assert.equal(fileChangeClass(undefined), '');
  });

  it('keeps an unstructured body as the prompt', () => {
    assert.deepEqual(splitMemoryBody('Use tokio spawn_blocking for file IO.'), {
      prompt: 'Use tokio spawn_blocking for file IO.',
      filesNote: null,
      commits: null,
      changes: null,
      request: null,
      conversation: null,
      outcome: null,
    });
  });

  it('reads commit hashes and places the diff popup left of the blade', () => {
    assert.deepEqual(commitsForMemory('turn-1', 'title', '- 9183e498c Keep the hook.'), [
      { hash: '9183e498c', subject: 'Keep the hook.' },
    ]);
    assert.deepEqual(commitsForMemory('git-012d783b6814', 'useCallOnce', null), [
      { hash: '012d783b6814', subject: 'useCallOnce' },
    ]);
    assert.equal(diffLineClass('added'), 'memory-diff-line memory-diff-line--added');
    assert.equal(diffLineClass('removed'), 'memory-diff-line memory-diff-line--removed');
    const placed = placeDiffPopup({ top: 200, left: 900, bottom: 224 }, { width: 1200, height: 800 }, 560);
    assert.ok(placed.left < 900);
    assert.equal(placed.width, 560);
    assert.equal(placed.top, 200);
    const low = placeDiffPopup({ top: 740, left: 980, bottom: 764 }, { width: 1200, height: 800 }, 560, 420);
    assert.ok(low.top >= 48);
    assert.ok(low.top + low.maxHeight <= 800 - 36);
    const centered = centerDiffPopup({ width: 1200, height: 800 }, 980, 720);
    assert.equal(centered.left, Math.round((1200 - centered.width) / 2));
    assert.ok(centered.top >= 48);
    assert.ok(centered.top + centered.maxHeight <= 800 - 36);
    assert.ok(centered.width > 560);
    assert.equal(shouldDismissDiffPopup({ onCommit: true, insidePopup: false }), false);
    assert.equal(shouldDismissDiffPopup({ onCommit: false, insidePopup: true }), false);
    assert.equal(shouldDismissDiffPopup({ onCommit: false, insidePopup: false }), true);
    assert.deepEqual(gitFileLinkProps('https://dev.azure.com/org/project/_git/repo?path=%2Fa'), {
      href: 'https://dev.azure.com/org/project/_git/repo?path=%2Fa',
      target: '_blank',
      rel: 'noopener noreferrer',
    });
    assert.equal(gitFileLinkProps(undefined), null);
  });

  it('returns an empty prompt for a blank body', () => {
    assert.deepEqual(splitMemoryBody('  \n\n  '), {
      prompt: '',
      filesNote: null,
      commits: null,
      changes: null,
      request: null,
      conversation: null,
      outcome: null,
    });
  });
});

describe('memoryImageUrl', () => {
  it('points local absolute images at the memory image endpoint, encoded', () => {
    assert.equal(
      memoryImageUrl('turn-1', '/var/folders/x/page 1.png'),
      '/api/memory/turn-1/image?src=%2Fvar%2Ffolders%2Fx%2Fpage%201.png',
    );
    assert.equal(
      memoryImageUrl('a/b', 'file:///tmp/w.webp'),
      '/api/memory/a%2Fb/image?src=file%3A%2F%2F%2Ftmp%2Fw.webp',
    );
    assert.equal(memoryImageUrl('m', 'C:\\shots\\a.png'), '/api/memory/m/image?src=C%3A%5Cshots%5Ca.png');
    assert.equal(memoryImageUrl('m', 'D:/shots/a.png'), '/api/memory/m/image?src=D%3A%2Fshots%2Fa.png');
  });

  it('leaves web, data, protocol-relative and relative images as written', () => {
    for (const src of ['https://x.io/a.png', 'http://x.io/a.png', 'data:image/png;base64,AA', '//cdn.io/a.png', 'shots/a.png', '']) {
      assert.equal(memoryImageUrl('m', src), null, src);
    }
  });
});

describe('memoryImageCount', () => {
  it('counts markdown images, with or without alt text', () => {
    assert.equal(memoryImageCount('a ![x](/a.png) b ![](https://x.io/b.jpg)'), 2);
  });

  it('does not count plain links or bodies without images', () => {
    assert.equal(memoryImageCount('[log](/a.png) and text'), 0);
    assert.equal(memoryImageCount(''), 0);
  });
});

describe('zoomAt', () => {
  const at = (v: { scale: number; x: number; y: number }, px: number, py: number) => ({
    cx: (px - v.x) / v.scale,
    cy: (py - v.y) / v.scale,
  });

  it('keeps the content point under the cursor in place', () => {
    const start = { scale: 1, x: 0, y: 0 };
    const next = zoomAt(start, 2, 100, 50);
    assert.equal(next.scale, 2);
    assert.deepEqual(at(next, 100, 50), at(start, 100, 50));
    const again = zoomAt(next, 1.5, 30, 70);
    assert.equal(again.scale, 3);
    assert.deepEqual(at(again, 30, 70), at(next, 30, 70));
  });

  it('clamps the scale at the maximum without moving the point', () => {
    const v = zoomAt({ scale: 6, x: -50, y: -20 }, 4, 40, 40);
    assert.equal(v.scale, 8);
    assert.deepEqual(at(v, 40, 40), at({ scale: 6, x: -50, y: -20 }, 40, 40));
  });

  it('clamps at the minimum and centers again there', () => {
    assert.deepEqual(zoomAt({ scale: 2, x: -100, y: -40 }, 0.25, 10, 10), { scale: 1, x: 0, y: 0 });
  });
});
