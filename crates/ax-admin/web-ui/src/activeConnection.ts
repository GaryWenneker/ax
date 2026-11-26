import { createContext, useContext } from 'react';

/** The environment the pages are showing. */
export interface ActiveConnection {
  id: string;
  name: string;
  /**
   * Another environment became active elsewhere and a call was refused: load
   * the application again and show `message` so the user knows why.
   */
  changed: (message: string) => void;
}

export const ActiveConnectionContext = createContext<ActiveConnection | null>(null);

export function useActiveConnection(): ActiveConnection {
  const connection = useContext(ActiveConnectionContext);
  if (!connection) throw new Error('This page needs a connected environment');
  return connection;
}
