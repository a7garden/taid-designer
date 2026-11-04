import { useCallback, useEffect, useRef, useState } from 'react';
import { AlertCircle, ArrowLeft, ArrowRight, Check, ChevronDown, Copy, FolderOpen, FolderOutput, ImagePlus, Layers, LoaderCircle, Minus, Plus, RefreshCw, Search, Settings2, Square, X } from 'lucide-react';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { getCurrentWindow } from '@tauri-apps/api/window';
import type { AppSettings, CodexModel, RecentProject, Workspace } from './domain';
import * as api from './bridge';
import { ReviewPanel } from './components/ReviewPanel';
import type { ReviewRequest } from './components/ReviewPanel';
import { Handoff } from './components/Handoff';
import { ModelSettings, modelValidation } from './components/ModelSettings';
import { Preview } from './components/Preview';
import { Profiles } from './components/Profiles';
import { errorMessage, Field, useModalKeyboard, isApproved } from './components/ui';
import { useImagePaste } from './useImagePaste';
import './styles.css';

type Screen = 'studio' | 'rules' | 'handoff';
type Filter = 'all' | 'review' | 'issues' | 'approved' | 'reference';
type ProjectAction = { kind: 'open' | 'create' | 'reload' | 'close' } | { kind: 'recent'; project: RecentProject };
const isMac = /Mac|iPhone|iPad/.test(navigator.platform);
const isWindows = /Win/.test(navigator.platform);
export default function App() {
  const [workspace, setWorkspace] = useState<Workspace | null>(null);
  const workspaceRef = useRef<Workspace | null>(null);
  const [workspaceSession, setWorkspaceSession] = useState(0);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState('');
  const busyRef = useRef(false);
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const [screen, setScreen] = useState<Screen>('studio');
  const [activeId, setActiveId] = useState('');
  const [selectedIds, setSelectedIds] = useState<string[]>([]);
  const [search, setSearch] = useState('');
  const [filter, setFilter] = useState<Filter>('all');
  const [importProfileId, setImportProfileId] = useState('');
  const [dragging, setDragging] = useState(false);
  const [createModal, setCreateModal] = useState(false);
  const [projectName, setProjectName] = useState('');
  const [createError, setCreateError] = useState('');
  const [queuedPaths, setQueuedPaths] = useState<string[]>([]);
  const [autoReview, setAutoReview] = useState(false);
  const [reviewQueue, setReviewQueue] = useState<ReviewRequest[]>([]);
  const reviewRequestId = useRef(0);
  const [pendingSwitch, setPendingSwitch] = useState<ProjectAction | null>(null);
  const [rulesDirty, setRulesDirty] = useState(false);
  const [reviewDirty, setReviewDirty] = useState(false);
  const [aiRunning, setAiRunning] = useState(false);
  const [projectMenu, setProjectMenu] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);
  const [recent, setRecent] = useState<RecentProject[]>([]);
  const [recentLoading, setRecentLoading] = useState(false);
  const [recentError, setRecentError] = useState('');
  const [externalChanged, setExternalChanged] = useState(false);
  const [syncError, setSyncError] = useState('');
  const [settingsModal, setSettingsModal] = useState(false);
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [settingsDraft, setSettingsDraft] = useState<AppSettings | null>(null);
  const [settingsLoading, setSettingsLoading] = useState(true);
  const [settingsError, setSettingsError] = useState('');
  const [models, setModels] = useState<CodexModel[]>([]);
  const [modelsLoading, setModelsLoading] = useState(false);
  const [modelsLoaded, setModelsLoaded] = useState(false);
  const [modelsError, setModelsError] = useState('');
  const modelsRequested = useRef(false);
  const modelsLoadingRef = useRef(false);
  const [windowMaximized, setWindowMaximized] = useState(false);
  const closeApproved = useRef(false);
  const closeState = useRef({ dirty: false, aiRunning: false });
  const cancelCreate = () => {
    if (busyRef.current) return;
    const paths = queuedPaths;
    setCreateModal(false); setQueuedPaths([]);
    void api.releaseClipboardImages(paths).catch(error => setError(errorMessage(error)));
  };
  useModalKeyboard(createModal && !pendingSwitch, cancelCreate);
  useModalKeyboard(!!pendingSwitch, () => { if (!busyRef.current) setPendingSwitch(null); });
  useModalKeyboard(settingsModal && !pendingSwitch, () => { if (!busyRef.current) setSettingsModal(false); });
  const dirty = rulesDirty || reviewDirty;
  const settingsDirty = !!settingsDraft && !!settings && (settingsDraft.model !== settings.model || settingsDraft.effort !== settings.effort);
  closeState.current = { dirty: dirty || settingsDirty, aiRunning };
  const reportError = useCallback((message: string) => setError(message), []);
  const handleRulesDirty = useCallback((value: boolean) => setRulesDirty(value), []);
  const handleReviewDirty = useCallback((value: boolean) => setReviewDirty(value), []);
  const handleRequestHandled = useCallback((id: number) => setReviewQueue(queue => queue.filter(request => request.id !== id)), []);
  const mayCloseWindow = useCallback(() => {
    if (closeApproved.current) { closeApproved.current = false; return true; }
    if (closeState.current.dirty || closeState.current.aiRunning || busyRef.current) {
      setProjectMenu(false); setPendingSwitch({ kind: 'close' }); return false;
    }
    return true;
  }, []);
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    try {
      getCurrentWindow().onCloseRequested(event => {
        if (disposed || !mayCloseWindow()) event.preventDefault();
      }).then(stop => { if (disposed) stop(); else unlisten = stop; }).catch(e => { if (!disposed) reportError(`창 닫기 보호를 설정하지 못했습니다: ${errorMessage(e)}`); });
    } catch (e) { reportError(errorMessage(e)); }
    return () => { disposed = true; unlisten?.(); };
  }, [mayCloseWindow, reportError]);
  useEffect(() => {
    if (!isWindows) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    const currentWindow = getCurrentWindow();
    const updateMaximized = () => {
      void currentWindow.isMaximized().then(value => { if (!disposed) setWindowMaximized(value); }).catch(e => { if (!disposed) reportError(errorMessage(e)); });
    };
    currentWindow.onResized(updateMaximized).then(stop => { if (disposed) stop(); else { unlisten = stop; updateMaximized(); } }).catch(e => { if (!disposed) reportError(errorMessage(e)); });
    return () => { disposed = true; unlisten?.(); };
  }, [reportError]);
  async function runWindowCommand(command: 'minimize' | 'toggleMaximize' | 'close') {
    if (command === 'close' && !mayCloseWindow()) return;
    try {
      const currentWindow = getCurrentWindow();
      await currentWindow[command]();
      if (command === 'toggleMaximize') setWindowMaximized(await currentWindow.isMaximized());
    } catch (e) { reportError(errorMessage(e)); }
  }
  const acceptWorkspace = useCallback((next: Workspace) => {
    const previous = workspaceRef.current;
    const sameProject = previous?.root === next.root;
    const ids = new Set(next.assets.map(a => a.id));
    const oldIds = new Set(sameProject && previous ? previous.assets.map(a => a.id) : []);
    const added = sameProject ? next.assets.filter(a => !oldIds.has(a.id)).map(a => a.id) : [];
    workspaceRef.current = next;
    setWorkspace(next);
    setSelectedIds(old => added.length ? added : sameProject ? old.filter(id => ids.has(id)) : []);
    setActiveId(old => added[0] ?? (sameProject && ids.has(old) ? old : next.assets[0]?.id ?? ''));
    setImportProfileId(old => next.profiles.some(p => p.id === old) ? old : next.profiles[0]?.id ?? '');
    if (!sameProject) { setRulesDirty(false); setReviewDirty(false); setReviewQueue([]); setSearch(''); setFilter('all'); setScreen('studio'); setExternalChanged(false); setSyncError(''); }
  }, []);
  const runAction = useCallback(async (label: string, action: () => Promise<void>): Promise<boolean> => {
    if (busyRef.current) return false;
    busyRef.current = true; setBusy(label); setNotice('');
    try { await action(); if (label.includes('복사')) setNotice('클립보드에 복사했습니다.'); return true; }
    catch (e) { reportError(errorMessage(e)); return false; }
    finally { busyRef.current = false; setBusy(''); }
  }, [reportError]);
  const runWorkspace = useCallback((label: string, action: () => Promise<Workspace>) => runAction(label, async () => acceptWorkspace(await action())), [runAction, acceptWorkspace]);
  const loadRecent = useCallback(async () => {
    setRecentLoading(true); setRecentError('');
    try { setRecent(await api.listRecentProjects()); }
    catch (e) { setRecentError(errorMessage(e)); }
    finally { setRecentLoading(false); }
  }, []);
  const loadSettings = useCallback(async () => {
    setSettingsLoading(true); setSettingsError('');
    try { const saved = await api.getSettings(); setSettings(saved); setSettingsDraft(saved); }
    catch (e) { setSettingsError(errorMessage(e)); }
    finally { setSettingsLoading(false); }
  }, []);
  const loadModels = useCallback(async () => {
    if (modelsLoadingRef.current) return;
    modelsLoadingRef.current = true; modelsRequested.current = true; setModelsLoading(true); setModelsError('');
    try { setModels(await api.codexModels()); setModelsLoaded(true); }
    catch (e) { setModelsError(errorMessage(e)); }
    finally { modelsLoadingRef.current = false; setModelsLoading(false); }
  }, []);
  useEffect(() => {
    let live = true;
    api.currentWorkspace().then(current => { if (live && current) acceptWorkspace(current); }).catch(e => { if (live) reportError(errorMessage(e)); }).finally(() => { if (live) setLoading(false); });
    void loadRecent(); void loadSettings();
    return () => { live = false; };
  }, [acceptWorkspace, reportError, loadRecent, loadSettings]);
  useEffect(() => { if (settingsModal && !modelsRequested.current) void loadModels(); }, [settingsModal, loadModels]);
  useEffect(() => {
    if (!projectMenu) return;
    void loadRecent();
    const close = (event: PointerEvent) => { if (!menuRef.current?.contains(event.target as Node)) setProjectMenu(false); };
    const escape = (event: KeyboardEvent) => { if (event.key === 'Escape') { setProjectMenu(false); menuRef.current?.querySelector('button')?.focus(); } };
    document.addEventListener('pointerdown', close); document.addEventListener('keydown', escape);
    return () => { document.removeEventListener('pointerdown', close); document.removeEventListener('keydown', escape); };
  }, [projectMenu, loadRecent]);
  useEffect(() => {
    if (!workspace?.root) return;
    let live = true, checking = false;
    const root = workspace.root;
    const check = async () => {
      if (checking || busyRef.current || document.hidden) return;
      checking = true;
      try { const changed = await api.workspaceChanged(); if (live && workspaceRef.current?.root === root) { if (changed) setExternalChanged(true); setSyncError(''); } }
      catch (e) { if (live) setSyncError(errorMessage(e)); }
      finally { checking = false; }
    };
    const interval = window.setInterval(() => void check(), 5000);
    window.addEventListener('focus', check);
    return () => { live = false; window.clearInterval(interval); window.removeEventListener('focus', check); };
  }, [workspace?.root]);
  const importIntoWorkspace = useCallback(async (paths: string[], current: Workspace, profileId: string, review: boolean) => {
    const before = new Set(current.assets.map(asset => asset.id));
    const next = await api.importAssets(paths, profileId || current.profiles[0]?.id || '');
    const addedIds = next.assets.filter(asset => !before.has(asset.id)).map(asset => asset.id);
    acceptWorkspace(next);
    setScreen('studio');
    setNotice(`${addedIds.length}개 추가 · 로컬 파일 검사 완료${review && addedIds.length ? addedIds.length > 8 ? ' · AI 검토는 한 번에 최대 8개' : ' · AI 검토 요청' : ' · AI 전송 안 함'}`);
    if (review && addedIds.length) setReviewQueue(queue => [...queue, { id: ++reviewRequestId.current, assetIds: addedIds }]);
  }, [acceptWorkspace]);
  const prepareFirstImport = useCallback((paths: string[]) => {
    setQueuedPaths(paths);
    const filename = paths[0]?.split(/[\\/]/).pop() ?? '';
    setProjectName(/^Clipboard_.*_paste\.png$/i.test(filename) ? '클립보드 검토' : paths.length === 1 ? filename.replace(/\.(png|jpe?g)$/i, '') : '');
    setCreateError('');
    setCreateModal(true);
  }, []);
  const importPaths = useCallback(async (paths: string[]) => {
    if (!paths.length) return;
    if (busyRef.current) { await api.releaseClipboardImages(paths); reportError('현재 작업이 끝난 뒤 이미지를 다시 놓아 주세요.'); return; }
    const current = workspaceRef.current;
    if (!current) { prepareFirstImport(paths); return; }
    const imported = await runAction('PNG 추가 · 로컬 파일 검사', () => importIntoWorkspace(paths, current, importProfileId, autoReview));
    if (!imported) await api.releaseClipboardImages(paths);
  }, [importProfileId, autoReview, reportError, runAction, importIntoWorkspace, prepareFirstImport]);
  const pasteImage = useImagePaste({
    onPaths: importPaths, onError: reportError,
    disabled: !!busy || loading || createModal || settingsModal || !!pendingSwitch,
  });
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    try {
      getCurrentWebview().onDragDropEvent(event => {
        if (event.payload.type === 'enter' || event.payload.type === 'over') setDragging(true);
        if (event.payload.type === 'leave') setDragging(false);
        if (event.payload.type === 'drop') { setDragging(false); void importPaths(event.payload.paths); }
      }).then(stop => { if (disposed) stop(); else unlisten = stop; }).catch(e => { if (!disposed) reportError(errorMessage(e)); });
    } catch (e) { reportError(errorMessage(e)); }
    return () => { disposed = true; unlisten?.(); };
  }, [importPaths, reportError]);
  const chooseImport = useCallback(() => runAction('PNG 파일 선택', async () => {
    const paths = await api.chooseImages();
    if (!paths.length) return;
    const current = workspaceRef.current;
    if (!current) { prepareFirstImport(paths); return; }
    await importIntoWorkspace(paths, current, importProfileId, autoReview);
  }), [runAction, importProfileId, autoReview, importIntoWorkspace, prepareFirstImport]);
  const applyProject = useCallback((next: Workspace) => {
    acceptWorkspace(next); setWorkspaceSession(session => session + 1); setRulesDirty(false); setReviewDirty(false); setReviewQueue([]); setExternalChanged(false); setSyncError(''); void loadRecent();
  }, [acceptWorkspace, loadRecent]);
  const executeProjectAction = useCallback((action: ProjectAction) => {
    setProjectMenu(false);
    if (action.kind === 'close') {
      if (closeState.current.aiRunning || busyRef.current) { setPendingSwitch(action); return; }
      closeApproved.current = true;
      void getCurrentWindow().close().catch(e => { closeApproved.current = false; reportError(errorMessage(e)); });
      return;
    }
    if (action.kind === 'create') { setQueuedPaths([]); setProjectName(''); setCreateError(''); setCreateModal(true); return; }
    void runAction(action.kind === 'reload' ? '프로젝트 다시 불러오기' : '프로젝트 열기', async () => {
      if (action.kind === 'reload') { applyProject(await api.reloadWorkspace()); return; }
      const path = action.kind === 'recent' ? action.project.root : await api.chooseProject(false);
      if (path) applyProject(await api.openWorkspace(path));
    });
  }, [runAction, applyProject, reportError]);
  const requestSwitch = useCallback((action: ProjectAction) => {
    if (busyRef.current) return;
    if (aiRunning) { setProjectMenu(false); reportError('실행 중인 Codex 검토를 마치거나 이미지 옆 검토 패널에서 취소한 뒤 프로젝트를 바꿔 주세요.'); return; }
    if (action.kind === 'recent' && !action.project.available) { setProjectMenu(false); reportError(`프로젝트 폴더를 찾을 수 없습니다: ${action.project.root}\n폴더가 이동했거나 드라이브 연결이 해제되었는지 확인한 뒤 ‘프로젝트 열기’로 다시 선택하세요. 현재 프로젝트와 최근 목록은 유지됩니다.`); return; }
    setProjectMenu(false);
    if (dirty || settingsDirty) setPendingSwitch(action);
    else executeProjectAction(action);
  }, [dirty, settingsDirty, executeProjectAction, aiRunning, reportError]);
  useEffect(() => {
    function keydown(event: KeyboardEvent) {
      if (createModal || pendingSwitch || settingsModal || document.querySelector('[role="dialog"]:not([hidden])')) return;
      if (!(event.metaKey || event.ctrlKey) || event.altKey || event.shiftKey) return;
      if (event.key.toLowerCase() === 'o') { event.preventDefault(); requestSwitch({ kind: 'open' }); }
      if (event.key.toLowerCase() === 'i') { event.preventDefault(); void chooseImport(); }
    }
    window.addEventListener('keydown', keydown);
    return () => window.removeEventListener('keydown', keydown);
  }, [requestSwitch, chooseImport, createModal, pendingSwitch, settingsModal]);
  async function createProject() {
    if (!projectName.trim()) { setCreateError('프로젝트 이름을 입력해 주세요.'); return; }
    await runAction(queuedPaths.length ? '프로젝트 만들기 · PNG 추가 및 로컬 검사' : '프로젝트 만들기', async () => {
      const path = await api.chooseProject(true);
      if (!path) return;
      const next = await api.createWorkspace(path, projectName.trim());
      applyProject(next);
      setCreateModal(false);
      const paths = queuedPaths;
      setQueuedPaths([]);
      if (paths.length) await importIntoWorkspace(paths, next, next.profiles[0]?.id ?? '', autoReview);
    });
  }
  async function saveSettings() {
    if (!settingsDraft || modelValidation(settingsDraft, models)) return;
    setSettingsError('');
    await runAction('모델 설정 저장', async () => {
      try { const saved = await api.saveSettings({ ...settingsDraft, model: settingsDraft.model.trim() }); setSettings(saved); setSettingsDraft(saved); setNotice('모델 설정을 저장했습니다.'); }
      catch (e) { setSettingsError(errorMessage(e)); }
    });
  }
  const modelSettings = { saved: settings, draft: settingsDraft, models, loading: settingsLoading, catalogLoading: modelsLoading, catalogLoaded: modelsLoaded, error: settingsError, catalogError: modelsError, busy: !!busy || aiRunning, onChange: setSettingsDraft, onSave: () => void saveSettings(), onReload: () => void loadSettings(), onCatalog: () => void loadModels(), run: runAction };
  const active = workspace?.assets.find(a => a.id === activeId);
  const visible = workspace?.assets.filter(a => {
    const matches = `${a.name} ${a.sourceName} ${a.sequence}`.toLocaleLowerCase().includes(search.toLocaleLowerCase());
    return matches && (filter === 'all' || (filter === 'reference' ? a.reference : filter === 'approved' ? isApproved(a) : filter === 'issues' ? a.issues.length > 0 : !a.reference && !isApproved(a)));
  }) ?? [];
  const toggleSelected = (id: string, checked: boolean) => setSelectedIds(old => checked ? [...new Set([...old, id])] : old.filter(v => v !== id));
  const selectAsset = (id: string) => { setActiveId(id); setScreen('studio'); };
  const openModelSettings = () => { setProjectMenu(false); setSettingsModal(true); };
  const intakeOptions = <label className="auto-review-option"><input type="checkbox" role="switch" checked={autoReview} disabled={!!busy || loading} onChange={event => setAutoReview(event.target.checked)} /><span><strong>추가 후 AI 검토</strong><small>{autoReview ? 'Codex 전송 · 계정 사용량 소비' : '꺼짐 · 로컬 검사만 실행'}</small></span></label>;
  const intakeContent = <div className="intake-content">
    <span className="intake-filetype">PNG</span>
    <h1>이미지를 놓고 검토하세요</h1>
    <p>원본을 프로젝트에 보관하고 크기·투명도·픽셀 규격을 자동 검사합니다. AI 검토는 직접 실행하거나 ‘추가 후 AI 검토’를 켰을 때만 시작합니다.</p>
    <div className="form-actions"><button className="button primary" disabled={!!busy || loading} onClick={() => void chooseImport()}><ImagePlus size={16} />PNG 가져오기</button><button className="button" disabled={!!busy || loading} onClick={() => void pasteImage()}>클립보드 붙여넣기</button></div>
    <span className="intake-drop-hint">파일을 놓거나 {isMac ? '⌘V · Ctrl+V' : 'Ctrl+V'}로 이미지를 붙여넣으세요</span>
    {!workspace && intakeOptions}
    <ol className="intake-workflow"><li>PNG 추가</li><li>로컬 검사 <small>자동</small></li><li>AI 검토 <small>선택</small></li><li>정리·승인</li><li>전달</li></ol>
  </div>;
  return <div className={`app-shell ${isMac ? 'platform-mac' : isWindows ? 'platform-windows' : 'platform-other'}`}>
    <header className="app-header" data-tauri-drag-region>
      <div className="project-switcher" ref={menuRef}>
        <button className="project-trigger" aria-label="프로젝트 전환" aria-expanded={projectMenu} aria-controls="project-menu" title={workspace?.root ?? '프로젝트 열기 또는 만들기'} disabled={!!busy || loading} onClick={() => setProjectMenu(open => !open)}><Layers size={17} /><strong>{workspace?.name ?? 'Taid Designer'}</strong>{(dirty || settingsDirty) && <span className="dirty-dot" aria-label="저장 전 편집 있음" />}<ChevronDown size={13} /></button>
        {projectMenu && <div className="project-menu" id="project-menu" role="group" aria-label="프로젝트 메뉴">
          {workspace && <div className="current-project"><strong>{workspace.name}</strong><span>{workspace.root}</span></div>}
          <div className="project-menu-actions"><button disabled={aiRunning} onClick={() => requestSwitch({ kind: 'open' })}><FolderOpen size={15} />프로젝트 열기 <kbd>{isMac ? '⌘' : 'Ctrl+'}O</kbd></button><button disabled={aiRunning} onClick={() => requestSwitch({ kind: 'create' })}><Plus size={15} />새 프로젝트</button>{workspace && <button disabled={aiRunning} onClick={() => requestSwitch({ kind: 'reload' })}><RefreshCw size={15} />다시 불러오기</button>}</div>
          <div className="recent-heading"><span>최근 프로젝트</span><button className="icon-button" title="최근 프로젝트 새로고침" aria-label="최근 프로젝트 새로고침" disabled={recentLoading} onClick={() => void loadRecent()}><RefreshCw size={13} className={recentLoading ? 'spin' : ''} /></button></div>
          {recentError ? <p className="inline-error">최근 목록을 불러오지 못했습니다: {recentError}</p> : recent.length ? <div className="recent-projects">{recent.map(project => <button key={project.root} disabled={aiRunning} className={project.root === workspace?.root ? 'current' : ''} title={project.root} onClick={() => requestSwitch({ kind: 'recent', project })}><span><strong>{project.name}</strong><small>{project.root}</small></span>{!project.available ? <span className="unavailable-project">찾을 수 없음</span> : project.root === workspace?.root ? <Check size={14} /> : null}</button>)}</div> : <p className="menu-empty">{recentLoading ? '목록 불러오는 중…' : '최근 프로젝트가 없습니다.'}</p>}
        </div>}
      </div>
      {workspace && screen !== 'studio' && <button className="back-to-studio" onClick={() => setScreen('studio')}><ArrowLeft size={14} />이미지로 돌아가기</button>}
      <span className="titlebar-spacer" data-tauri-drag-region />
      {workspace && <button className={`quiet-header-action ${screen === 'rules' ? 'active' : ''}`} onClick={() => setScreen('rules')}>제작 기준</button>}
      <button className="model-trigger" title={settings ? `저장된 모델: ${settings.model || 'Codex 기본 모델'} · ${settings.effort}` : '모델 설정 열기'} aria-label="Codex 모델 설정 열기" onClick={openModelSettings}><Settings2 size={15} /><span>모델 설정</span>{settingsDirty && <span className="dirty-dot" />}</button>
      <div className="header-actions"><button className="button" disabled={!!busy || loading} onClick={() => void chooseImport()}><ImagePlus size={15} />PNG 추가</button>{workspace && <button className={`button ${screen === 'handoff' ? 'active' : 'quiet'}`} onClick={() => setScreen('handoff')}><FolderOutput size={15} />내보내기</button>}</div>
      {isWindows && <div className="window-controls" role="group" aria-label="창 제어">
        <button type="button" aria-label="창 최소화" title="최소화" onClick={() => void runWindowCommand('minimize')}><Minus size={14} /></button>
        <button type="button" aria-label={windowMaximized ? '창 이전 크기로 복원' : '창 최대화'} title={windowMaximized ? '이전 크기로 복원' : '최대화'} onClick={() => void runWindowCommand('toggleMaximize')}>{windowMaximized ? <Copy size={13} /> : <Square size={12} />}</button>
        <button type="button" className="window-close" aria-label="창 닫기" title="닫기" onClick={() => void runWindowCommand('close')}><X size={16} /></button>
      </div>}
    </header>
    {externalChanged && <div className="external-change" role="status"><RefreshCw size={14} /><span>외부에서 변경됨</span><button disabled={!!busy || aiRunning} onClick={() => requestSwitch({ kind: 'reload' })}>다시 불러오기</button>{dirty && <small>저장 전 편집값이 있습니다.</small>}</div>}
    {error && <div className="error-banner" role="alert"><AlertCircle size={17} /><p>{error}</p><button className="icon-button" aria-label="오류 메시지 닫기" onClick={() => setError('')}><X size={16} /></button></div>}
    {syncError && <div className="sync-warning"><button onClick={() => reportError(`외부 변경 여부를 확인하지 못했습니다: ${syncError}`)}>외부 변경 확인 실패 · 자세히</button></div>}
    {loading ? <main className="welcome"><LoaderCircle className="spin" size={22} /><p>프로젝트 불러오는 중…</p></main> : !workspace ? <main className="welcome">
      <div className="welcome-content">{intakeContent}
        <div className="welcome-project-actions"><button disabled={!!busy} onClick={() => requestSwitch({ kind: 'open' })}><FolderOpen size={15} />기존 프로젝트 열기</button><button disabled={!!busy} onClick={() => requestSwitch({ kind: 'create' })}><Plus size={15} />빈 프로젝트 만들기</button></div>
        {recent.length > 0 && <div className="welcome-recents"><h2>최근 프로젝트</h2>{recent.slice(0, 3).map(project => <button key={project.root} disabled={!!busy} title={project.root} onClick={() => requestSwitch({ kind: 'recent', project })}><FolderOpen size={15} /><span>{project.name}</span>{!project.available ? <small>찾을 수 없음</small> : <ArrowRight size={14} />}</button>)}</div>}
        {recentError && <p className="inline-error">최근 목록을 불러오지 못했습니다: {recentError}</p>}
      </div>
    </main> : <div className="workspace-screens" key={`${workspace.root}:${workspaceSession}`}>
      <div hidden={screen !== 'studio'} className="screen studio-screen">
        <aside className="asset-sidebar">
          <div className="asset-sidebar-heading"><h2>이미지</h2><span>{workspace.assets.length}</span><button className="icon-button" aria-label="PNG 추가" title="PNG 추가" disabled={!!busy} onClick={() => void chooseImport()}><Plus size={16} /></button></div>
          <div className="asset-search"><Search size={14} /><input aria-label="에셋 검색" value={search} onChange={e => setSearch(e.target.value)} placeholder="파일·시퀀스 검색" /></div>
          <div className="asset-filter"><select aria-label="에셋 상태 필터" value={filter} onChange={e => setFilter(e.target.value as Filter)}><option value="all">모든 이미지</option><option value="review">승인 대기</option><option value="issues">로컬 검사 문제</option><option value="approved">승인 완료</option><option value="reference">참고용</option></select></div>
          <div className="selection-toolbar"><label><input type="checkbox" aria-label="보이는 에셋 모두 선택" ref={element => { if (element) element.indeterminate = visible.some(a => selectedIds.includes(a.id)) && !visible.every(a => selectedIds.includes(a.id)); }} checked={visible.length > 0 && visible.every(a => selectedIds.includes(a.id))} disabled={!visible.length || !!busy} onChange={e => { const shown = new Set(visible.map(a => a.id)); setSelectedIds(old => e.target.checked ? [...new Set([...old, ...shown])] : old.filter(id => !shown.has(id))); }} />{selectedIds.length ? `${selectedIds.length}개 선택` : '전체 선택'}</label>{selectedIds.length > 0 && <button className="text-button" disabled={!!busy} onClick={() => setSelectedIds([])}>해제</button>}</div>
          <div className="asset-list">{visible.map(a => <div key={a.id} className={`asset-row ${activeId === a.id ? 'active' : ''}`}><input type="checkbox" aria-label={`${a.name} 선택`} checked={selectedIds.includes(a.id)} onChange={e => toggleSelected(a.id, e.target.checked)} disabled={!!busy} /><button className="asset-row-main" aria-current={activeId === a.id ? 'true' : undefined} onClick={() => setActiveId(a.id)}><span className="asset-thumbnail"><img src={api.imageUrl(a.filePath, a.currentHash)} alt="" /></span><span className="asset-row-info"><strong title={a.name}>{a.name}</strong><small className={a.reference ? 'reference' : isApproved(a) ? 'approved' : a.issues.length ? 'issue' : ''}>{a.reference ? '참고용' : isApproved(a) ? '승인 완료' : a.issues.length ? `로컬 문제 ${a.issues.length}` : '로컬 검사 완료'}</small></span></button></div>)}{!visible.length && <div className="asset-list-empty">{workspace.assets.length ? '검색 결과가 없습니다.' : '추가한 이미지가 여기에 표시됩니다.'}</div>}</div>
          <div className="intake-settings"><div className="import-profile"><label htmlFor="import-profile">추가할 이미지의 제작 기준</label><select id="import-profile" value={importProfileId} disabled={!!busy} onChange={e => setImportProfileId(e.target.value)}>{workspace.profiles.map(p => <option key={p.id} value={p.id}>{p.name}</option>)}</select></div>{intakeOptions}</div>
        </aside>
        {workspace.assets.length ? <Preview workspace={workspace} asset={active} onActive={setActiveId} /> : <section className="preview-region intake-canvas">{intakeContent}</section>}
        <ReviewPanel workspace={workspace} asset={active} selectedIds={selectedIds} busy={!!busy} run={runWorkspace} reportError={reportError} onRunning={setAiRunning} onDirty={handleReviewDirty} request={reviewQueue[0] ?? null} onRequestHandled={handleRequestHandled} onOpenModelSettings={openModelSettings} />
      </div>
      <div hidden={screen !== 'rules'} className="screen"><Profiles workspace={workspace} busy={!!busy} run={runWorkspace} runAction={runAction} onDirty={handleRulesDirty} onSelectAsset={selectAsset} /></div>
      <div hidden={screen !== 'handoff'} className="screen"><Handoff workspace={workspace} selectedIds={selectedIds} setSelectedIds={setSelectedIds} busy={!!busy} run={runAction} /></div>
    </div>}
    {(busy || notice) && <div className="activity-toast" role="status">{busy ? <><LoaderCircle size={14} className="spin" /><span>{busy}</span></> : <><Check size={14} /><span>{notice}</span><button className="icon-button" aria-label="알림 닫기" onClick={() => setNotice('')}><X size={13} /></button></>}</div>}
    {dragging && <div className="drop-overlay"><ImagePlus size={30} /><strong>PNG를 놓아 검토 시작</strong><span>{workspace ? '원본 보관 · 로컬 파일 자동 검사' : '저장할 프로젝트를 만든 뒤 이미지를 추가합니다.'}</span><small>{autoReview ? '추가 후 Codex AI 검토 실행' : 'AI에 전송하지 않습니다.'}</small></div>}
    {settingsModal && !pendingSwitch && <div className="modal-backdrop"><section className="modal settings-modal" role="dialog" aria-modal="true" aria-labelledby="settings-title"><div className="modal-heading"><div><h2 id="settings-title">Codex 모델 설정</h2><p>저장한 설정은 다음 AI 검토에 적용됩니다.</p></div><button className="icon-button" aria-label="모델 설정 닫기" onClick={() => setSettingsModal(false)}><X size={17} /></button></div><ModelSettings {...modelSettings} /></section></div>}
    {createModal && !pendingSwitch && <div className="modal-backdrop"><form className="modal create-modal" role="dialog" aria-modal="true" aria-labelledby="create-title" onSubmit={e => { e.preventDefault(); void createProject(); }}><div className="modal-heading"><h2 id="create-title">{queuedPaths.length ? '이미지를 보관할 프로젝트' : '새 프로젝트'}</h2><button className="icon-button" type="button" aria-label="프로젝트 만들기 취소" disabled={!!busy} onClick={cancelCreate}><X size={17} /></button></div>{queuedPaths.length > 0 && <p className="queued-import"><ImagePlus size={16} /><span>선택한 PNG {queuedPaths.length}개 · 만든 뒤 자동으로 추가</span></p>}<p className="helper">선택한 위치에 프로젝트 전용 폴더를 만듭니다. 원본과 기존 파일은 덮어쓰지 않습니다.</p><Field label="프로젝트 이름"><input autoFocus value={projectName} onChange={e => { setProjectName(e.target.value); setCreateError(''); }} disabled={!!busy} /></Field>{createError && <p className="inline-error" role="alert">{createError}</p>}<div className="form-actions"><button className="button primary" type="submit" disabled={!!busy}><FolderOpen size={15} />저장 위치 선택 · 만들기</button><button className="button quiet" type="button" disabled={!!busy} onClick={cancelCreate}>취소</button></div></form></div>}
    {pendingSwitch && <div className="modal-backdrop"><section className="modal" role="dialog" aria-modal="true" aria-labelledby="unsaved-title">
      <h2 id="unsaved-title">{pendingSwitch.kind === 'close' && aiRunning ? 'Codex 검토가 실행 중입니다' : pendingSwitch.kind === 'close' && busy ? '작업을 처리하고 있습니다' : pendingSwitch.kind === 'close' && !closeState.current.dirty ? '앱을 닫을까요?' : '저장 전 편집값이 있습니다'}</h2>
      <p>{pendingSwitch.kind === 'close' && aiRunning ? '이미지 옆 검토 패널에서 실행을 마치거나 취소한 뒤 닫아 주세요. 앱을 닫기 위해 검토를 자동 취소하지 않습니다.' : pendingSwitch.kind === 'close' && busy ? '현재 작업이 끝난 뒤 창을 닫을 수 있습니다.' : pendingSwitch.kind === 'close' ? closeState.current.dirty ? '앱을 닫으면 저장하지 않은 리소스 정보·제작 기준·AI 요청문·모델 설정 편집값이 사라집니다. 저장한 원본과 결과는 유지됩니다.' : '현재 작업이 끝났습니다.' : `${pendingSwitch.kind === 'reload' ? '다시 불러오면' : '프로젝트를 바꾸면'} 저장하지 않은 리소스 정보·제작 기준·AI 요청문·모델 설정 편집값이 사라집니다. 저장한 원본과 결과는 유지됩니다.`}</p>
      <div className="form-actions"><button className={`button ${pendingSwitch.kind === 'close' && !closeState.current.dirty ? 'primary' : 'danger'}`} disabled={!!busy || aiRunning} onClick={() => { const action = pendingSwitch; setPendingSwitch(null); if (settingsDirty) setSettingsDraft(settings); executeProjectAction(action); }}>{pendingSwitch.kind === 'close' ? closeState.current.dirty ? '편집값을 버리고 닫기' : '앱 닫기' : '편집값을 버리고 계속'}</button><button className="button" autoFocus onClick={() => setPendingSwitch(null)}>돌아가기</button></div>
    </section></div>}
  </div>;
}
