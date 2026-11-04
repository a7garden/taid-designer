import { useCallback, useEffect, useRef, useState } from 'react';
import { AlertTriangle, Check, CheckCheck, Copy, RefreshCw, Send, Settings2, Square } from 'lucide-react';
import ReactMarkdown from 'react-markdown';
import type { AppSettings, Asset, AutomationPlan, CodexModel, CodexStatus, Workspace } from '../domain';
import * as api from '../bridge';
import { buildReviewPrompt, reviewAssets, reviewScopeError, reviewSnapshot } from '../review';
import { Inspector } from './Inspector';
import { CutoutControl } from './CutoutControl';
import { PaletteTools } from './PaletteTools';
import { TileTools } from './TileTools';
import { Empty, errorMessage, Field, formatNumber } from './ui';
import { Choice, DetailPanel } from './Interaction';
import type { WorkspaceAction } from './ui';

export interface ReviewRequest { id: number; assetIds: string[]; }
type Phase = 'connecting' | 'streaming' | 'completed' | 'cancelled' | 'error';
type Review = { ids: string[]; names: string[]; snapshot: string; text: string; phase: Phase; activity: string; model: string; };
type Props = {
  workspace: Workspace; asset?: Asset; selectedIds: string[]; busy: boolean; run: WorkspaceAction;
  reportError: (message: string) => void; onRunning: (running: boolean) => void; onDirty: (dirty: boolean) => void;
  request: ReviewRequest | null; onRequestHandled: (id: number) => void; onOpenModelSettings: () => void;
};

export function ReviewPanel({ workspace, asset, selectedIds, busy, run, reportError, onRunning, onDirty, request, onRequestHandled, onOpenModelSettings }: Props) {
  const [tab, setTab] = useState<'review' | 'resource' | 'tools'>('review');
  const [tool, setTool] = useState<'pixel' | 'tile' | 'palette' | 'cutout'>('pixel');
  const [tileDirty, setTileDirty] = useState(false);
  const [status, setStatus] = useState<CodexStatus | null>(null);
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [models, setModels] = useState<CodexModel[]>([]);
  const [connectionError, setConnectionError] = useState('');
  const [listenerError, setListenerError] = useState('');
  const [listening, setListening] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  const [running, setRunning] = useState(false);
  const [cancelBusy, setCancelBusy] = useState(false);
  const [question, setQuestion] = useState('');
  const [sentQuestion, setSentQuestion] = useState('');
  const [inspectorDirty, setInspectorDirty] = useState(false);
  const [paletteDirty, setPaletteDirty] = useState(false);
  const [review, setReview] = useState<Review | null>(null);
  const [notice, setNotice] = useState('');
  const [copyNotice, setCopyNotice] = useState('');
  const [preparation, setPreparation] = useState<{ workspace: Workspace; ids: string; plan?: AutomationPlan; error?: string }>();
  const [preparationNotice, setPreparationNotice] = useState('');
  const mounted = useRef(false);
  const runningRef = useRef(false);
  const refreshingRef = useRef(false);
  const cancelRef = useRef(false);
  const handledRequests = useRef(new Set<number>());
  const errorHandler = useRef(reportError);
  errorHandler.current = reportError;

  const refresh = useCallback(async () => {
    if (refreshingRef.current || runningRef.current) return;
    refreshingRef.current = true;
    setRefreshing(true);
    try {
      const [nextStatus, nextSettings] = await Promise.all([api.codexStatus(), api.getSettings()]);
      if (!mounted.current) return;
      setStatus(nextStatus);
      setSettings(nextSettings);
      setConnectionError('');
      if (nextStatus.installed && nextStatus.authenticated && !nextStatus.busy && !nextStatus.error) {
        try {
          const nextModels = await api.codexModels();
          if (mounted.current) setModels(nextModels);
        } catch {
          // The saved model ID remains an exact label when catalog lookup is unavailable.
          if (mounted.current) setModels([]);
        }
      }
    } catch (error) {
      if (mounted.current) { setConnectionError(errorMessage(error)); errorHandler.current(errorMessage(error)); }
    } finally {
      refreshingRef.current = false;
      if (mounted.current) setRefreshing(false);
    }
  }, []);

  useEffect(() => {
    mounted.current = true;
    let disposed = false;
    let dispose: (() => void) | undefined;
    void refresh();
    api.onCodexEvent(event => {
      if (disposed || !runningRef.current) return;
      if (event.kind === 'text') setReview(previous => previous ? { ...previous, text: previous.text + event.text, phase: 'streaming' } : previous);
      else if (event.kind === 'error') {
        setReview(previous => previous ? { ...previous, phase: 'error', activity: event.text } : previous);
      } else {
        // A done event is activity only; the command result determines the terminal state.
        setReview(previous => previous ? { ...previous, activity: event.text } : previous);
      }
    }).then(unlisten => {
      if (disposed) unlisten();
      else { dispose = unlisten; setListening(true); }
    }).catch(error => {
      if (!disposed) { setListenerError(errorMessage(error)); errorHandler.current(errorMessage(error)); }
    });
    const focus = () => { if (!runningRef.current) void refresh(); };
    window.addEventListener('focus', focus);
    return () => { disposed = true; mounted.current = false; dispose?.(); window.removeEventListener('focus', focus); };
  }, [refresh]);
  useEffect(() => { onRunning(running || !!status?.busy); }, [running, status?.busy, onRunning]);
  useEffect(() => { onDirty(inspectorDirty || paletteDirty || tileDirty || question !== sentQuestion); }, [inspectorDirty, paletteDirty, tileDirty, question, sentQuestion, onDirty]);

  const targetIds = selectedIds.length ? selectedIds : asset ? [asset.id] : [];
  const targetKey = JSON.stringify(targetIds);
  const currentPreparation = preparation?.workspace === workspace && preparation.ids === targetKey ? preparation : undefined;
  const preparationPlan = currentPreparation?.plan;
  const preparationSteps = [...new Set(preparationPlan?.items.flatMap(item => item.steps) ?? [])];
  const preparationWarnings = [...new Set(preparationPlan?.items.flatMap(item => item.warnings) ?? [])];
  useEffect(() => {
    if (targetKey === '[]') return;
    let cancelled = false;
    api.automationPlan(JSON.parse(targetKey) as string[]).then(plan => {
      if (!cancelled) setPreparation({ workspace, ids: targetKey, plan });
    }).catch(error => {
      if (!cancelled) setPreparation({ workspace, ids: targetKey, error: errorMessage(error) });
    });
    return () => { cancelled = true; };
  }, [workspace, targetKey]);
  const targets = reviewAssets(workspace, targetIds);
  const scopeError = reviewScopeError(workspace, targetIds);
  const modelLabel = settings ? `${models.find(model => model.id === settings.model)?.displayName || settings.model || 'Codex 기본 모델'} · ${settings.effort}` : '모델 설정 확인 중';
  const unavailable = !status || !status.installed || !status.authenticated || !!status.error || !!connectionError || !settings || !listening || !!listenerError;
  const stale = !!review && review.snapshot !== reviewSnapshot(workspace, review.ids);
  const includesActive = !!asset && !!review?.ids.includes(asset.id);
  const activeReview = includesActive && !stale ? review : null;
  const errors = asset?.issues.filter(issue => issue.severity === 'error') ?? [];
  const warnings = asset?.issues.filter(issue => issue.severity === 'warning') ?? [];
  const approvalMissing = targets.length !== targetIds.length;
  const references = targets.filter(item => item.reference).length;
  const approvalErrors = targets.filter(item => item.issues.some(issue => issue.severity === 'error')).length;
  const allDesign = targets.length > 0 && targets.every(item => item.designApproved);
  const allQc = targets.length > 0 && targets.every(item => item.qcApproved);
  const approvalBlocked = !targets.length || approvalMissing || references > 0;
  const qcReason = approvalMissing ? '선택 대상 일부를 찾을 수 없습니다.' : references ? `참고용 ${references}개를 선택에서 제외해야 승인할 수 있습니다.` : !targets.length ? '승인할 이미지를 선택하세요.' : !allDesign ? '선택한 모든 이미지의 디자인을 먼저 승인하세요.' : approvalErrors ? `${approvalErrors}개 이미지에 파일 검사 오류가 남아 있어 QC 승인할 수 없습니다.` : '';
  const stateLabel = !asset ? '이미지를 선택하세요' : stale && includesActive ? '이전 AI 검토 · 변경 후 재검토 필요' : activeReview ? ({ connecting: 'AI 연결 중', streaming: 'AI 검토 중', completed: 'AI 검토 완료', cancelled: 'AI 검토 취소됨', error: 'AI 검토 실패' } as const)[activeReview.phase] : '파일 검사 완료 · AI 검토 전';

  const startReview = useCallback(async (ids: string[], requestId?: number) => {
    if (runningRef.current || busy || status?.busy) return;
    const consume = () => {
      if (requestId !== undefined && !handledRequests.current.has(requestId)) {
        handledRequests.current.add(requestId);
        onRequestHandled(requestId);
      }
    };
    const invalid = reviewScopeError(workspace, ids);
    const cannotConnect = connectionError || listenerError || status?.error || (!status?.installed ? 'Codex CLI를 설치한 뒤 연결 상태를 새로고침하세요.' : !status.authenticated ? '터미널에서 codex login을 실행한 뒤 연결 상태를 새로고침하세요.' : !settings || !listening ? 'Codex 연결과 모델 설정을 확인한 뒤 다시 검토하세요.' : '');
    if (invalid || cannotConnect) { setNotice(invalid || cannotConnect); consume(); return; }
    const images = reviewAssets(workspace, ids);
    const prompt = buildReviewPrompt(workspace, images, question);
    if (new TextEncoder().encode(prompt).length > 32000) {
      setNotice('이미지별 제작 기준·메모·추가 질문이 Codex 요청 한도(32,000바이트)를 넘었습니다. 전송하지 않았습니다. 메모를 줄이거나 더 적은 이미지를 선택하세요.');
      consume();
      return;
    }
    runningRef.current = true;
    setRunning(true);
    setNotice('');
    setCopyNotice('');
    setTab('review');
    setSentQuestion(question);
    setReview({ ids: [...ids], names: images.map(item => `${item.name}.png`), snapshot: reviewSnapshot(workspace, ids), text: '', phase: 'connecting', activity: '저장된 모델 설정 확인 중', model: '' });
    consume();
    try {
      const saved = await api.getSettings();
      if (!mounted.current) return;
      setSettings(saved);
      setReview(previous => previous ? { ...previous, model: `${models.find(model => model.id === saved.model)?.displayName || saved.model || 'Codex 기본 모델'} · ${saved.effort}`, activity: 'Codex 연결 중 · 이미지 전송 요청' } : previous);
      const result = await api.startCodex(prompt, images.map(item => item.filePath));
      if (!mounted.current) return;
      setReview(previous => previous ? { ...previous, text: result.text || previous.text, phase: result.status === 'completed' ? 'completed' : 'cancelled', activity: result.status === 'completed' ? '검토 완료 · 자동 변경·승인 없음' : '검토 취소됨 · 사용된 계정 할당량은 반환되지 않습니다' } : previous);
    } catch (error) {
      if (mounted.current) {
        const message = errorMessage(error);
        setReview(previous => previous ? { ...previous, phase: 'error', activity: message } : previous);
        errorHandler.current(message);
      }
    } finally {
      runningRef.current = false;
      cancelRef.current = false;
      if (mounted.current) {
        setRunning(false);
        setCancelBusy(false);
        await refresh();
      }
    }
  }, [busy, status, workspace, connectionError, listenerError, settings, listening, question, onRequestHandled, models, refresh]);

  useEffect(() => {
    if (!request || handledRequests.current.has(request.id) || running || runningRef.current || busy || refreshing || status?.busy) return;
    if ((!status || !settings) && !connectionError) return;
    if (!listening && !listenerError) return;
    void startReview(request.assetIds, request.id);
  }, [request, running, busy, refreshing, status, settings, connectionError, listening, listenerError, startReview]);

  async function cancel() {
    if (cancelRef.current || (!runningRef.current && !status?.busy)) return;
    cancelRef.current = true;
    setCancelBusy(true);
    try {
      await api.cancelCodex();
      if (mounted.current) setReview(previous => previous ? { ...previous, activity: '취소 요청 전달됨 · 종료 확인 중' } : previous);
      if (!runningRef.current) await refresh();
    } catch (error) {
      if (mounted.current) errorHandler.current(errorMessage(error));
    } finally {
      cancelRef.current = false;
      if (mounted.current) setCancelBusy(false);
    }
  }
  async function copy() {
    if (!review?.text) return;
    try { await api.copyText(review.text); if (mounted.current) setCopyNotice('검토 내용을 복사했습니다.'); }
    catch (error) { errorHandler.current(errorMessage(error)); }
  }
  async function saveReview() {
    if (!asset || !review || review.phase !== 'completed' || stale || !includesActive || inspectorDirty) return;
    const addition = `AI 검토 · ${review.names.join(', ')}\n모델: ${review.model}\n${review.text}`;
    if (await run('AI 검토를 현재 에셋 메모에 추가', () => api.updateAsset(asset.id, { notes: [asset.notes, addition].filter(Boolean).join('\n\n') }))) setCopyNotice('현재 에셋 메모에 저장했습니다. 메모 변경으로 QC 재승인이 필요합니다.');
  }

  async function autoPrepare() {
    if (busy || running || !preparationPlan || !preparationPlan.changedCount || preparationPlan.blocked.length) return;
    const count = preparationPlan.changedCount;
    if (await run('기준에 맞춰 정리', () => api.autoPrepareAssets(targetIds))) {
      setPreparationNotice(`새 미승인 결과 ${count}개 생성`);
    }
  }

  return <aside className="review-dock" aria-label="이미지 검토">
    <header className="review-heading"><h2 title={asset?.name}>{asset?.name ?? '에셋'}</h2>{asset && <span>{asset.width} × {asset.height}</span>}</header>
    <div className="review-tabs" role="tablist" aria-label="검토와 에셋 도구">
      {([['review', '검토'], ['tools', '정리'], ['resource', '정보']] as const).map(([id, label]) => <button key={id} id={`review-tab-${id}`} role="tab" aria-selected={tab === id} aria-controls={`review-panel-${id}`} className={tab === id ? 'active' : ''} onClick={() => setTab(id)}>{label}{((id === 'resource' && inspectorDirty) || (id === 'tools' && (inspectorDirty || paletteDirty || tileDirty))) && <span className="dirty-dot" aria-label="저장 전 편집 있음" />}</button>)}
    </div>
    <div className="review-body" id={`review-panel-${tab}`} role="tabpanel" aria-labelledby={`review-tab-${tab}`}>
      <section className="profile-preparation" hidden={tab === 'resource'} aria-label="기준에 맞춰 정리">
        <div className="review-section-heading"><h3>기준에 맞춰 정리</h3><span className="muted">{selectedIds.length ? '선택' : '현재'} {targetIds.length}개 · 로컬</span></div>
        {!targetIds.length ? <p className="helper">정리할 이미지를 선택하세요.</p> : !currentPreparation ? <p className="helper" role="status">저장된 제작 기준과 필요한 처리를 확인 중…</p> : currentPreparation.error ? <p className="inline-error" role="alert">{currentPreparation.error}</p> : preparationPlan && <>
          {preparationSteps.length > 0 && <p className="preparation-summary">감지된 {preparationSteps.length}가지 처리를 한 번에 적용합니다.</p>}
          {preparationWarnings.length > 0 && <p className="tool-caution">일부 도트·색·반투명 표현이 달라지거나 사라질 수 있습니다.</p>}
          {preparationPlan.changedCount > 0 && <DetailPanel title={`처리 계획 · 새 결과 ${preparationPlan.changedCount}개`}><ul className="preparation-items">{preparationPlan.items.map(item => <li key={item.assetId}><h3>{item.name}.png</h3><p>{item.outputWidth} × {item.outputHeight}px · {item.steps.join(' → ') || '변경 없음'}</p>{item.warnings.map(warning => <p key={warning}>{warning}</p>)}</li>)}</ul><p>원본·이전 결과·저장 전 편집은 유지합니다. 고립 도트와 그림 품질은 직접 검수하며 자동 승인하지 않습니다.</p></DetailPanel>}
          {preparationPlan.blocked.length > 0 && <ul className="preparation-blocked inline-error" role="alert">{preparationPlan.blocked.map((reason, index) => <li key={index}>{reason}</li>)}</ul>}
          {!preparationPlan.changedCount && !preparationPlan.blocked.length ? <p className="inspection-clear" role="status"><Check size={15} />설정된 규격에서 변경할 항목 없음</p> : <button className="button primary full" disabled={busy || running || !preparationPlan.changedCount || !!preparationPlan.blocked.length} onClick={() => void autoPrepare()}>기준에 맞춰 정리 · {preparationPlan.changedCount}개</button>}
          {preparationPlan.changedCount > 0 && <p className="helper">새 미승인 결과 · 원본 보존 · 이미지 전송 없음</p>}
        </>}
        {preparationNotice && <p className="tool-success" role="status">{preparationNotice}</p>}
      </section>
      <div hidden={tab !== 'review'}>
        {!asset ? <Empty title="이미지를 선택하세요"><p>가져온 파일의 로컬 검사와 AI 검토를 여기서 확인합니다.</p></Empty> : <section className="local-checks">
          <div className="review-section-heading"><h3>파일 검사</h3><span className="muted">로컬 · 자동</span></div>
          <p className="review-state" data-state={activeReview?.phase || 'local'} aria-live="polite">{stateLabel}</p>
          <p className="local-check-summary">{workspace.profiles.find(profile => profile.id === asset.profileId)?.name ?? '제작 기준 없음'}</p>
          {asset.issues.length ? <DetailPanel title={`${errors.length ? `오류 ${errors.length}개` : '오류 없음'}${warnings.length ? ` · 주의 ${warnings.length}개` : ''} · 검사 상세`}><ul className="issue-list">{asset.issues.map((issue, index) => <li key={`${issue.code}-${index}`} className={issue.severity}><div className="issue-title"><strong>{issue.severity === 'error' ? '오류' : '주의'}</strong><code>{issue.code}</code></div><p>{issue.message}</p>{issue.pixels > 0 && <span>{formatNumber(issue.pixels)} {issue.code === 'palette-variants' ? '색상 변형' : issue.code === 'isolated-dots' ? '논리 도트' : '픽셀'} · 로컬 측정</span>}</li>)}</ul></DetailPanel> : <p className="inspection-clear"><Check size={15} />파일 규칙 통과</p>}
          {errors.some(issue => issue.code === 'grid' || issue.code === 'alpha' || issue.code === 'palette') && <button className="button small" onClick={() => { setTool(errors.some(issue => issue.code === 'grid' || issue.code === 'alpha') ? 'pixel' : 'palette'); setTab('tools'); }}>정리 도구 열기</button>}
          <DetailPanel title="실제 픽셀 검사 수치"><p className="helper">크기·알파·팔레트·픽셀 규칙 검사입니다. 그림의 스타일이나 형태를 AI가 판단한 결과는 아닙니다.</p><dl className="metrics-list"><div><dt>사용 색상</dt><dd>{formatNumber(asset.metrics.colors)}</dd></div><div><dt>반투명 픽셀</dt><dd>{formatNumber(asset.metrics.semiTransparent)}</dd></div><div><dt>불투명 픽셀</dt><dd>{formatNumber(asset.metrics.opaque)}</dd></div><div><dt>그리드 이탈 픽셀</dt><dd>{formatNumber(asset.metrics.gridViolations)}</dd></div></dl></DetailPanel>
          <DetailPanel title="픽셀아트 검수 가이드"><ol><li>먼저 제작 기준의 도트 크기와 실제 격자를 맞춥니다.</li><li>승인 팔레트와 사용 색을 대조합니다. 색 수 자체가 합격·불합격 기준은 아닙니다.</li><li>고립 도트는 잡티 후보입니다. 눈·별·하이라이트·수동 안티앨리어싱은 의도한 표현일 수 있습니다.</li><li>1× 보기와 다른 에셋 위에서 비교합니다. 실제 게임 표시 크기는 Unity에서도 확인하세요.</li></ol><dl className="metrics-list"><div><dt>RGB444 근사 색상 변형</dt><dd>{formatNumber(asset.metrics.nearColorVariants ?? 0)}</dd></div><div><dt>고립 색상 도트 후보</dt><dd>{asset.metrics.orphanCheckApplied ? `${formatNumber(asset.metrics.isolatedColorDots ?? 0)} / ${formatNumber(asset.metrics.visibleLogicalDots ?? 0)}도트` : '격자 정리 후 검사'}</dd></div></dl><p className="helper">RGB444는 가까운 색을 묶는 근사 지표이며 지각적 유사도나 그림 품질 판정이 아닙니다. 참고: Imagine Dev 「이 픽셀아트, 사실 가짜입니다」.</p></DetailPanel>
        </section>}
        <section className="ai-review">
          <div className="review-section-heading"><h3>AI 이미지 검토</h3><button className="icon-button" disabled={busy || running || refreshing} aria-label="Codex 연결과 모델 상태 새로고침" title="연결 상태 새로고침" onClick={() => void refresh()}><RefreshCw size={14} /></button></div>
          <button className="review-model" disabled={running || busy} onClick={onOpenModelSettings}><Settings2 size={14} /><span>{modelLabel}</span></button>
          <div className="review-disclosure">
            <p>{selectedIds.length ? '선택' : '현재'} {targetIds.length}개와 저장된 기준을 Codex에 전송 · 계정 사용량 소비</p>
            <DetailPanel title="전송 내용·대상"><ul className="review-attachments">{targets.map((item, index) => <li key={`${item.id}-${index}`}><span>{index + 1}.</span> {item.name}.png <small>{workspace.profiles.find(profile => profile.id === item.profileId)?.name ?? '기준 없음'}</small></li>)}</ul><p className="helper">이미지·저장된 기준·파일명·해시·프레임·피벗·메모·파일 검사 수치·추가 질문을 기존 Codex 제공자에게 보냅니다. 저장 전 편집은 제외합니다. 사용량·과금은 기존 계정 설정을 따르며 파일 수정·생성·자동 승인은 없습니다.</p></DetailPanel>
          </div>
          <DetailPanel title={`추가 검토 질문${question ? ' · 입력됨' : ' (선택)'}`}><Field label="추가 질문" className="review-question"><textarea rows={2} value={question} disabled={running} onChange={event => setQuestion(event.target.value)} placeholder="예: 달리기 프레임 사이에 팔 길이가 달라 보이는지" /></Field>{question && <button className="button small" disabled={running} onClick={() => { setQuestion(''); setSentQuestion(''); }}>추가 질문 지우기</button>}</DetailPanel>
          <div className="review-connection">
            {!status && !connectionError && <p className="helper">Codex 설치·로그인 상태 확인 중…</p>}
            {status && !status.authenticated && <p className="helper">Codex 계정 연결 필요</p>}
            {status && !status.installed && <p className="inline-error">공식 Codex CLI를 설치한 뒤 새로고침하세요. 별도 API 키를 앱에 넣지 않습니다.</p>}
            {status?.installed && !status.authenticated && <p className="inline-error">터미널에서 <code>codex login</code>으로 연결한 뒤 위 새로고침을 누르세요. 앱이 대신 로그인하지 않습니다.</p>}
            {(connectionError || status?.error || listenerError) && <p className="inline-error" role="alert">{connectionError || status?.error || `Codex 응답 수신 연결 실패: ${listenerError}`}</p>}
          </div>
          {scopeError && targetIds.length > 0 && <p className="inline-error">{scopeError}</p>}
          {notice && <p className="inline-error" role="alert">{notice}</p>}
          {request && running && <p className="helper">가져온 이미지의 자동 검토가 대기 중입니다. 현재 요청이 끝나면 시작합니다.</p>}
          <div className="review-actions"><button className="button primary" disabled={busy || running || !!status?.busy || unavailable || !!scopeError || refreshing} onClick={() => void startReview(targetIds)}><Send size={15} />AI로 검토</button>{(running || status?.busy) && <button className="button" disabled={cancelBusy} onClick={() => void cancel()}><Square size={13} />{cancelBusy ? '취소 요청 중' : '검토 취소'}</button>}</div>
          {review && <section className="review-result" aria-label="AI 검토 응답" aria-busy={running}>
            <div className="review-result-header"><h4>{review.phase === 'completed' ? '검토 결과' : review.phase === 'cancelled' ? '취소된 검토' : review.phase === 'error' ? '완료하지 못한 검토' : '검토 중'}</h4><span>{review.names.length}개 이미지</span></div>
            <p className="helper">검토 대상: {review.names.join(', ')}{review.model ? ` · ${review.model}` : ''}</p>
            {stale && <p className="review-stale" role="status">검토 후 이미지·메타데이터·제작 기준 또는 메모가 바뀌었습니다. 아래는 이전 상태의 결과이며 현재 결과로 사용할 수 없습니다.</p>}
            {!includesActive && asset && <p className="review-stale">현재 열어 둔 {asset.name}.png는 이 검토의 대상이 아닙니다.</p>}
            <p className={review.phase === 'error' ? 'inline-error' : 'helper'} role="status">{review.activity}</p>
            <div className="review-markdown">{review.text ? <ReactMarkdown skipHtml components={{ a: ({ children }) => <span>{children}</span>, img: ({ alt }) => <span>{alt || '응답의 이미지 링크'}</span> }}>{review.text}</ReactMarkdown> : <p className="helper">{running ? 'Codex 응답을 기다리고 있습니다.' : '표시할 응답이 없습니다.'}</p>}</div>
            <div className="review-result-actions"><button className="button small" disabled={!review.text || running} onClick={() => void copy()}><Copy size={14} />결과 복사</button><button className="button small" disabled={busy || running || !review.text || review.phase !== 'completed' || stale || !includesActive || inspectorDirty} onClick={() => void saveReview()}>현재 에셋 메모에 추가</button></div>
            {inspectorDirty && <p className="helper">메모에 추가하려면 리소스·픽셀 정리의 편집을 먼저 저장하거나 취소하세요.</p>}
            {copyNotice && <p className="helper" role="status">{copyNotice}</p>}
            <p className="helper">응답은 이 화면에만 남습니다. ‘메모에 추가’로 저장한 내용만 프로젝트에 보관됩니다. AI 응답은 사람의 승인을 대신하지 않습니다.</p>
          </section>}
        </section>
        <section className="approval-section">
          <div className="review-section-heading"><h3>최종 승인</h3><span className="muted">{selectedIds.length ? '선택' : '현재'} {targetIds.length}개</span></div>
          <div className="approval-steps">
            <button className={`button approval-button ${allDesign ? 'approved' : ''}`} disabled={busy || approvalBlocked} onClick={() => void run('선택 이미지 디자인 승인 변경', () => api.approveAssets(targetIds, !allDesign, false))}><Check size={15} />{allDesign ? '디자인 승인됨' : '디자인 승인'}</button>
            <button className={`button approval-button ${allQc ? 'approved' : ''}`} disabled={busy || approvalBlocked || (!allQc && !!qcReason)} onClick={() => void run('선택 이미지 QC 승인 변경', () => api.approveAssets(targetIds, true, !allQc))}><CheckCheck size={15} />{allQc ? 'QC 승인됨' : 'QC 승인'}</button>
          </div>
          {qcReason && <p className="helper compact">{qcReason}</p>}
          <DetailPanel title="승인 대상·안내"><p className="helper">{targets.map(item => `${item.name}.png`).join(', ') || '이미지를 선택하세요.'}</p><p className="helper">그림·움직임과 파일 규칙을 각각 확인하세요. 승인된 버튼을 다시 누르면 취소합니다. 디자인 승인을 바꾸면 QC 승인이 해제됩니다. 참고용 이미지는 승인·전달에서 제외합니다.</p></DetailPanel>
        </section>
      </div>
      <div className="tool-picker" hidden={tab !== 'tools'}><Choice label="수동 정리 도구" value={tool} onChange={setTool} options={[{ value: 'pixel', label: '도트 정리' }, { value: 'tile', label: '크기 맞춤' }, { value: 'palette', label: '색상 맞춤' }, { value: 'cutout', label: '배경 제거' }]} /></div>
      <div className="review-advanced" hidden={tab !== 'resource' && (tab !== 'tools' || tool !== 'pixel')}>
        <Inspector workspace={workspace} asset={asset} selectedIds={selectedIds} busy={busy || running} run={run} onDirty={setInspectorDirty} mode={tab === 'resource' ? 'resource' : 'process'} />
      </div>
      <div hidden={tab !== 'tools' || tool !== 'tile'}><TileTools workspace={workspace} asset={asset} selectedIds={selectedIds} busy={busy || running} run={run} reportError={reportError} onDirty={setTileDirty} /></div>
      <div hidden={tab !== 'tools' || tool !== 'palette'}><PaletteTools workspace={workspace} asset={asset} selectedIds={selectedIds} busy={busy || running} run={run} reportError={reportError} onDirty={setPaletteDirty} /></div>
      <div hidden={tab !== 'tools' || tool !== 'cutout'}><CutoutControl ids={targetIds} busy={busy || running} run={run} reportError={reportError} /></div>
    </div>
  </aside>;
}
