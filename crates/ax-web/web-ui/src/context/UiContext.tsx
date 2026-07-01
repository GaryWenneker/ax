import { createContext, useContext, useEffect, useState, type ReactNode } from 'react';

export interface PageContext {
  view: string;
  detail?: string;
}

interface UiContextValue {
  pageContext: PageContext;
  setPageContext: (ctx: PageContext) => void;
}

const defaultCtx: PageContext = { view: 'Stats' };

const UiContext = createContext<UiContextValue>({
  pageContext: defaultCtx,
  setPageContext: () => {},
});

export function UiProvider({ children }: { children: ReactNode }) {
  const [pageContext, setPageContext] = useState<PageContext>(defaultCtx);
  return (
    <UiContext.Provider value={{ pageContext, setPageContext }}>
      {children}
    </UiContext.Provider>
  );
}

export function useUiContext() {
  return useContext(UiContext);
}

/** Register active page label + optional detail for the status bar center cluster. */
export function usePageContext(view: string, detail?: string) {
  const { setPageContext } = useUiContext();
  useEffect(() => {
    setPageContext({ view, detail: detail || undefined });
  }, [view, detail, setPageContext]);
}
