import { useEffect, useRef, useState } from 'react';
import { Scissors, Square } from 'lucide-react';
import type { CutoutProgress, CutoutStatus } from '../domain';
import * as api from '../bridge';
import type { WorkspaceAction } from './ui';
import { errorMessage } from './ui';

export function CutoutControl({ ids, busy, run, reportError }: { ids: string[]; busy: boolean; run: WorkspaceAction; reportError: (message: string) => void }) {
  const [status, setStatus] = useState<CutoutStatus | null>(null);
  const [progress, setProgress] = useState<CutoutProgress | null>(null);
  const [running, setRunning] = useState(false);
  const [cancelling, setCancelling] = useState(false);
  const [statusError, setStatusError] = useState('');
  const [done, setDone] = useState(false);
  const alive = useRef(false);
  const active = useRef(false);
  useEffect(() => {
    alive.current = true;
    let disposed = false, unlisten: (() => void) | undefined;
    api.cutoutStatus().then(value => { if (!disposed) setStatus(value); }).catch(error => { if (!disposed) setStatusError(errorMessage(error)); });
    api.onCutoutProgress(value => { if (!disposed && active.current) setProgress(value); }).then(stop => { if (disposed) stop(); else unlisten = stop; }).catch(error => { if (!disposed) setStatusError(errorMessage(error)); });
    return () => { disposed = true; alive.current = false; unlisten?.(); };
  }, []);
  async function start() {
    if (!ids.length || busy || active.current) return;
    active.current = true; setRunning(true); setProgress(null); setStatusError(''); setDone(false);
    try { const ok = await run('배경 제거 · 로컬 처리', () => api.removeBackground(ids)); if (alive.current) setDone(ok); }
    finally {
      active.current = false;
      if (alive.current) {
        setRunning(false); setCancelling(false);
        api.cutoutStatus().then(setStatus).catch(error => setStatusError(errorMessage(error)));
      }
    }
  }
  async function cancel() {
    setCancelling(true);
    try { await api.cancelCutout(); }
    catch (error) { reportError(errorMessage(error)); setCancelling(false); }
  }
  const megabytes = (bytes: number) => `${(bytes / 1048576).toFixed(1)} MiB`;
  const activity = !progress ? '원본과 로컬 모델을 확인하고 있습니다'
    : progress.stage === 'download' ? `모델 다운로드 ${megabytes(progress.downloaded)}${progress.totalBytes ? ` / ${megabytes(progress.totalBytes)}` : ''}`
    : progress.stage === 'load-model' ? '로컬 모델을 준비하고 있습니다'
    : progress.stage === 'segment' ? `배경 분리 ${progress.completed} / ${progress.total}`
    : progress.stage === 'commit' ? '새 이미지로 저장하고 있습니다'
    : progress.stage === 'done' ? '새 누끼 이미지를 만들었습니다'
    : '원본과 로컬 모델을 확인하고 있습니다';
  return <section className="cutout-control" aria-label="로컬 배경 제거">
    <h3>배경 제거</h3>
    <p className="tool-intro">주요 피사체를 분리해 새 PNG로 남깁니다.</p>
    <p className="cutout-local">로컬 처리 · 이미지 전송 없음</p>
    {status && !status.ready && <p className="helper">첫 실행 시 {status.model} {megabytes(status.downloadBytes)} 다운로드 · {status.license}</p>}
    <div className="cutout-actions"><button className="button primary full" disabled={busy || running || !ids.length || !status} onClick={() => void start()}><Scissors size={15} />{running ? '배경 제거 중…' : `${ids.length}개 배경 제거`}</button>{running && <button className="button small" disabled={cancelling} onClick={() => void cancel()}><Square size={12} />{cancelling ? '중단 요청됨' : '중단'}</button>}</div>
    {running && <p className="helper" role="status">{activity}</p>}
    {done && <p className="tool-success" role="status">새 이미지를 만들었습니다. 원본은 유지됩니다.</p>}
    <p className="action-consequence">가는 선과 도트 경계는 결과를 확인하세요. 새 이미지는 미승인 상태입니다.</p>
    {statusError && <p className="inline-error" role="alert">{statusError}</p>}
    <details className="cutout-details"><summary>모델·처리 방식</summary><p className="helper">{status?.notice || '로컬 신경망으로 주요 피사체를 분리합니다. 가는 선·머리카락·픽셀 경계는 결과를 직접 확인하세요.'}</p><p className="helper">원본 RGB와 기존 투명도를 보존합니다. 도트 격자나 알파 규칙이 어긋나면 새 이미지에서 도트 정리를 실행하세요.</p></details>
  </section>;
}
