/// <reference types="vite/client" />

declare module '*.json' {
  const value: Array<{ id: string; label: string; order: number; aliases?: string[] }>;
  export default value;
}
