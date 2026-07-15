export type Edition = 'community' | 'full';
export type EditorMode = 'source' | 'split' | 'preview';

export interface DocumentTab {
  id: string;
  name: string;
  path: string | null;
  content: string;
  savedContent: string;
  fingerprint: DocumentFingerprint | null;
  conflict: boolean;
  recovered: boolean;
  selection: { anchor: number; head: number };
}

export interface DocumentFingerprint {
  modifiedMs: number;
  size: number;
  hash: string;
}

export interface FileDocument {
  path: string;
  name: string;
  content: string;
  fingerprint: DocumentFingerprint;
}

export interface WorkspaceEntry {
  path: string;
  relativePath: string;
  name: string;
}

export type Theme = 'system' | 'dark' | 'light' | 'contrast';

export interface AppSettings {
  version: 1;
  autosave: boolean;
  autosaveDelayMs: 500 | 1500 | 3000;
  restoreSession: boolean;
  keepDraftsSilently: boolean;
  theme: Theme;
  glassEffects: 'system' | 'on' | 'off';
  fontSize: number;
  lineWrap: boolean;
  showLineNumbers: boolean;
  tabSize: 2 | 4;
  previewFont: 'sans' | 'serif' | 'mono';
  spellcheck: boolean;
  focusMode: boolean;
}

export interface SessionState {
  version: 1;
  activeId: string | null;
  mode: EditorMode;
  workspaceRoot: string;
  tabs: Array<Omit<DocumentTab, 'fingerprint'> & { fingerprint: DocumentFingerprint | null }>;
  recentFiles: string[];
  recentWorkspaces: string[];
}

export const defaultSettings: AppSettings = {
  version: 1,
  autosave: true,
  autosaveDelayMs: 1500,
  restoreSession: true,
  keepDraftsSilently: false,
  theme: 'dark',
  glassEffects: 'system',
  fontSize: 14,
  lineWrap: true,
  showLineNumbers: true,
  tabSize: 4,
  previewFont: 'sans',
  spellcheck: false,
  focusMode: false,
};
