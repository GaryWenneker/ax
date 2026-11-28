export type ReplicationDialogs = { setup: boolean; restart: boolean; remove: boolean; stop: boolean };
export type ReplicationErrorSlot = 'page' | keyof ReplicationDialogs;

/** Where the replication error is shown: inside the open dialog, or on the page when no dialog covers it. */
export function replicationErrorSlot(open: ReplicationDialogs): ReplicationErrorSlot {
  if (open.setup) return 'setup';
  if (open.restart) return 'restart';
  if (open.remove) return 'remove';
  if (open.stop) return 'stop';
  return 'page';
}
