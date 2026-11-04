import { useEffect, useRef } from 'react';
import type { ReactNode } from 'react';
import { Check, Circle, AlertTriangle, ShieldCheck } from 'lucide-react';
import type { Asset, RuleProfile, Workspace } from '../domain';

export type RunAction = (label: string, action: () => Promise<void>) => Promise<boolean>;
export type WorkspaceAction = (label: string, action: () => Promise<Workspace>) => Promise<boolean>;

export function Field({ label, hint, children, className = '' }: { label: string; hint?: string; children: ReactNode; className?: string }) {
  return <label className={`field ${className}`}><span className="field-label">{label}</span>{children}{hint && <span className="field-hint">{hint}</span>}</label>;
}
export function AssetStatus({ asset }: { asset: Asset }) {
  if (asset.reference) return <span className="status reference"><Circle size={12} />참고용</span>;
  if (asset.designApproved && asset.qcApproved) return <span className="status approved"><ShieldCheck size={13} />승인 완료</span>;
  if (asset.issues.some(i => i.severity === 'error')) return <span className="status issue"><AlertTriangle size={12} />수정 필요</span>;
  if (asset.designApproved || asset.qcApproved) return <span className="status partial"><Check size={12} />일부 승인</span>;
  return <span className="status"><Circle size={11} />검토 대기</span>;
}
export function Empty({ title, children, icon }: { title: string; children?: ReactNode; icon?: ReactNode }) {
  return <div className="empty-state">{icon && <span className="empty-icon">{icon}</span>}<h2>{title}</h2>{children && <div className="empty-description">{children}</div>}</div>;
}
export function errorMessage(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === 'string') return error;
  return JSON.stringify(error) || '작업을 완료하지 못했습니다.';
}
export const isApproved = (a: Asset) => !a.reference && a.designApproved && a.qcApproved;
export const formatNumber = (value: number) => value.toLocaleString('ko-KR');
export function buildPrompt(profile: RuleProfile | undefined, notes: string): string {
  return [
    '다음 제작 기준에 따라 첨부 에셋을 검토하고, 어긋난 부분과 수정 지시를 한국어로 정리해 주세요. 원본을 수정하거나 파일을 생성하지 마세요.',
    profile ? `제작 기준: ${profile.name}\n단계: ${profile.stage || '미지정'} / 종류: ${profile.kind}\n논리 픽셀: ${profile.pixelSize}px\n지정 크기: ${profile.width ?? '자유'} × ${profile.height ?? '자유'}px\n알파: ${profile.alphaMode === 'binary' ? `이진 / 임계값 ${profile.alphaThreshold}` : '원본 알파 유지'}\n팔레트: ${profile.palette.length ? profile.palette.join(', ') : '제한 없음'}\n기준 메모:\n${profile.notes || '없음'}` : '선택한 제작 기준 없음',
    `프로젝트 공통 메모:\n${notes || '없음'}`,
    'Stage03의 4×4 논리 픽셀과 1672×944 배경 규격은 해당 제작 기준에만 적용합니다. 아틀라스 제작과 Unity 파일 자동 수정은 하지 않습니다.',
  ].join('\n\n');
}

export function useModalKeyboard(open: boolean, onClose: () => void) {
  const close = useRef(onClose);
  close.current = onClose;
  useEffect(() => {
    if (!open) return;
    const dialog = Array.from(document.querySelectorAll<HTMLElement>('[role="dialog"]')).filter(el => !el.closest('[hidden]') && el.getClientRects().length > 0).at(-1);
    if (!dialog) return;
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const focusable = () => Array.from(dialog.querySelectorAll<HTMLElement>('button:not(:disabled), input:not(:disabled), select:not(:disabled), textarea:not(:disabled), [tabindex="0"]')).filter(element => element.getClientRects().length > 0);
    if (!dialog.contains(document.activeElement)) focusable()[0]?.focus();
    const keydown = (event: KeyboardEvent) => {
      if (Array.from(document.querySelectorAll<HTMLElement>('[role="dialog"]')).filter(el => !el.closest('[hidden]') && el.getClientRects().length > 0).at(-1) !== dialog) return;
      if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); close.current(); }
      if (event.key !== 'Tab') return;
      const items = focusable();
      if (!items.length) { event.preventDefault(); return; }
      const first = items[0], last = items[items.length - 1];
      if (event.shiftKey && (document.activeElement === first || !dialog.contains(document.activeElement))) { event.preventDefault(); last.focus(); }
      else if (!event.shiftKey && (document.activeElement === last || !dialog.contains(document.activeElement))) { event.preventDefault(); first.focus(); }
    };
    document.addEventListener('keydown', keydown, true);
    return () => { document.removeEventListener('keydown', keydown, true); previous?.focus(); };
  }, [open]);
}
