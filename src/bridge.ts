import { convertFileSrc, invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { open } from '@tauri-apps/plugin-dialog';
import type { AppSettings, AssetPatch, CodexEvent, CodexModel, CodexResult, CodexStatus, CutoutProgress, CutoutStatus, ExportResult, PaletteReport, ProcessSettings, RecentProject, RuleProfile, TileSettings, Workspace } from './domain';

export const copyText = (text: string) => invoke<void>('copy_text', { text });
export const captureClipboardImage = () => invoke<string>('capture_clipboard_image');
export const releaseClipboardImages = (paths: string[]) => invoke<void>('release_clipboard_images', { paths });
export const currentWorkspace = () => invoke<Workspace | null>('workspace_current');
export const createWorkspace = (path: string, name: string) => invoke<Workspace>('workspace_create', { path, name });
export const openWorkspace = (path: string) => invoke<Workspace>('workspace_open', { path });
export const importAssets = (paths: string[], profileId: string) => invoke<Workspace>('assets_import', { paths, profileId });
export const saveProfile = (profile: RuleProfile) => invoke<Workspace>('profile_save', { profile });
export const saveProjectNotes = (notes: string) => invoke<Workspace>('project_notes_save', { notes });
export const updateAsset = (assetId: string, patch: AssetPatch) => invoke<Workspace>('asset_update', { assetId, patch });
export const processAssets = (assetIds: string[], settings: ProcessSettings) => invoke<Workspace>('assets_process', { assetIds, settings });
export const resetAsset = (assetId: string) => invoke<Workspace>('asset_reset', { assetId });
export const approveAssets = (assetIds: string[], design: boolean, qc: boolean) => invoke<Workspace>('assets_approve', { assetIds, design, qc });
export const exportAssets = (path: string, assetIds: string[]) => invoke<ExportResult>('assets_export', { path, assetIds });
export const codexStatus = () => invoke<CodexStatus>('codex_status');
export const startCodex = (prompt: string, assetPaths: string[]) => invoke<CodexResult>('codex_start', { prompt, assetPaths });
export const cancelCodex = () => invoke<void>('codex_cancel');
export const onCodexEvent = (listener: (event: CodexEvent) => void) => listen<CodexEvent>('codex-event', e => listener(e.payload));
export const getSettings = () => invoke<AppSettings>('get_settings');
export const saveSettings = (settings: AppSettings) => invoke<AppSettings>('save_settings', { settings });
export const codexModels = () => invoke<CodexModel[]>('codex_models');
export const listRecentProjects = () => invoke<RecentProject[]>('list_recent_projects');
export const workspaceChanged = () => invoke<boolean>('workspace_changed');
export const reloadWorkspace = () => invoke<Workspace>('workspace_reload');
export const cutoutStatus = () => invoke<CutoutStatus>('cutout_status');
export const removeBackground = (assetIds: string[]) => invoke<Workspace>('remove_background', { assetIds });
export const cancelCutout = () => invoke<void>('cancel_cutout');
export const onCutoutProgress = (listener: (event: CutoutProgress) => void) => listen<CutoutProgress>('cutout-progress', event => listener(event.payload));
export const inspectPalette = (assetId: string) => invoke<PaletteReport>('asset_palette', { assetId });
export const mapPaletteAssets = (assetIds: string[]) => invoke<Workspace>('assets_palette_map', { assetIds });
export const readPalette = (path: string) => invoke<string[]>('palette_read', { path });
export const tileAssets = (assetIds: string[], settings: TileSettings) => invoke<Workspace>('assets_tile', { assetIds, settings });
export async function choosePalette(): Promise<string | null> {
  return await open({ multiple: false, directory: false, title: '팔레트 가져오기', filters: [{ name: '팔레트', extensions: ['hex', 'gpl'] }] }) as string | null;
}
export const imageUrl = (path: string, hash = '') => `${convertFileSrc(path)}?v=${encodeURIComponent(hash)}`;
export async function chooseProject(create: boolean): Promise<string | null> {
  return await open({ directory: true, multiple: false, title: create ? '새 작업 폴더 선택' : 'Taid 프로젝트 폴더 열기' }) as string | null;
}
export async function chooseImages(): Promise<string[]> {
  const result = await open({ multiple: true, directory: false, title: 'PNG 또는 JPEG 가져오기', filters: [{ name: '이미지', extensions: ['png', 'jpg', 'jpeg'] }] });
  return result === null ? [] : Array.isArray(result) ? result : [result];
}
export async function chooseExport(): Promise<string | null> {
  return await open({ directory: true, multiple: false, title: '전달 패키지를 저장할 상위 폴더' }) as string | null;
}
