export type AlphaMode = 'binary' | 'preserve';
export interface RuleProfile {
  id: string; name: string; stage: string; kind: 'sprite' | 'background' | 'ui' | 'effect';
  pixelSize: number; width: number | null; height: number | null;
  alphaMode: AlphaMode; alphaThreshold: number; palette: string[]; notes: string;
}
export interface Issue { code: string; severity: 'error' | 'warning'; message: string; pixels: number; }
export interface Metrics { colors: number; semiTransparent: number; opaque: number; gridViolations: number; nearColorVariants?: number; isolatedColorDots?: number; visibleLogicalDots?: number; orphanCheckApplied?: boolean; }
export interface Asset {
  id: string; name: string; sourceName: string; width: number; height: number;
  originalPath: string; filePath: string; originalHash: string; currentHash: string;
  profileId: string; sequence: string; frameIndex: number; durationMs: number;
  pivotX: number; pivotY: number; layer: number; notes: string; reference: boolean;
  cropX: number; cropY: number; processed: boolean;
  designApproved: boolean; qcApproved: boolean; issues: Issue[]; metrics: Metrics;
  parentId?: string | null; parentHash?: string | null;
}
export interface Workspace {
  schemaVersion: number; name: string; root: string; notes: string;
  profiles: RuleProfile[]; assets: Asset[];
}
export interface AssetPatch {
  name?: string; profileId?: string; sequence?: string; frameIndex?: number; durationMs?: number;
  pivotX?: number; pivotY?: number; layer?: number; notes?: string; reference?: boolean;
}
export interface ProcessSettings { snapToGrid: boolean; hardenAlpha: boolean; mapPalette: boolean; trim: boolean; scale: number; }
export interface ExportResult { path: string; count: number; }
export interface CodexStatus { installed: boolean; version: string; authenticated: boolean; account: string; busy: boolean; error: string | null; }
export interface CodexEvent { kind: 'text' | 'status' | 'error' | 'done'; text: string; }
export interface CodexResult { text: string; status: 'completed' | 'interrupted'; }
export interface AppSettings { model: string; effort: 'none' | 'minimal' | 'low' | 'medium' | 'high' | 'xhigh'; }
export interface CodexModel { id: string; displayName: string; description: string; efforts: string[]; }
export interface RecentProject { root: string; name: string; lastOpened: string; available: boolean; }
export interface CutoutStatus { ready: boolean; busy: boolean; model: string; license: string; licenseUrl: string; downloadBytes: number; revision: string; notice: string; }
export interface CutoutProgress { stage: string; completed: number; total: number; downloaded: number; totalBytes: number | null; }
export interface PaletteColor { hex: string; pixels: number; frequency: number; }
export interface PaletteOutside extends PaletteColor { nearestApproved: string | null; distanceSquared: number | null; }
export interface PaletteReport { assetId: string; inputHash: string; visiblePixels: number; colorCount: number; approvedPaletteSize: number; outsideColors: number; outsidePixels: number; dominantColors: PaletteColor[]; outsideSamples: PaletteOutside[]; allColors: PaletteColor[] | null; }
export interface TileSettings { width: number; height: number; scale: number; mode: 'pad' | 'fit'; anchor: 'center' | 'bottom-center'; }
export interface AutomationItem { assetId: string; name: string; steps: string[]; warnings: string[]; outputWidth: number; outputHeight: number; }
export interface AutomationPlan { items: AutomationItem[]; changedCount: number; blocked: string[]; }
