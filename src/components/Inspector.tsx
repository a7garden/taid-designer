import { useEffect, useState } from 'react';
import { Check, RotateCcw, Save, SlidersHorizontal } from 'lucide-react';
import type { Asset, AssetPatch, ProcessSettings, Workspace } from '../domain';
import * as api from '../bridge';
import { Empty, Field } from './ui';
import { Choice, DetailPanel, ToggleAction } from './Interaction';
import type { WorkspaceAction } from './ui';
import '../workbench-tools.css';

type Draft = { name: string; profileId: string; sequence: string; frameIndex: string; durationMs: string; pivotX: string; pivotY: string; layer: string; notes: string; reference: boolean };
type Entry = { value: Draft; edits: Partial<Draft> };
type ProcessDraft = { value: ProcessSettings; saved: ProcessSettings };
const defaultProcessing = (): ProcessSettings => ({ snapToGrid: false, hardenAlpha: false, mapPalette: false, trim: false, scale: 1 });
function draftFor(asset: Asset): Draft {
  return { name: asset.name, profileId: asset.profileId, sequence: asset.sequence, frameIndex: String(asset.frameIndex), durationMs: String(asset.durationMs), pivotX: String(asset.pivotX), pivotY: String(asset.pivotY), layer: String(asset.layer), notes: asset.notes, reference: asset.reference };
}
function processingChanged(entry: ProcessDraft): boolean {
  return (Object.keys(entry.value) as (keyof ProcessSettings)[]).some(key => entry.value[key] !== entry.saved[key]);
}

export function Inspector({ workspace, asset, selectedIds, busy, run, onDirty, mode }: {
  workspace: Workspace; asset?: Asset; selectedIds: string[]; busy: boolean; run: WorkspaceAction; onDirty: (dirty: boolean) => void; mode: 'resource' | 'process';
}) {
  const [drafts, setDrafts] = useState<Record<string, Entry>>({});
  const [processDrafts, setProcessDrafts] = useState<Record<string, ProcessDraft>>({});
  const [validation, setValidation] = useState('');
  const [resetConfirm, setResetConfirm] = useState(false);
  const [applied, setApplied] = useState<Record<string, { settings: ProcessSettings; ids: string[] }>>({});
  const [savedAsset, setSavedAsset] = useState('');
  useEffect(() => {
    setDrafts(previous => Object.fromEntries(workspace.assets.map(item => {
      const edits = previous[item.id]?.edits ?? {};
      return [item.id, { value: { ...draftFor(item), ...edits }, edits }];
    })));
    setProcessDrafts(previous => Object.fromEntries(workspace.assets.flatMap(item => previous[item.id] ? [[item.id, previous[item.id]]] : [])));
  }, [workspace.assets]);
  useEffect(() => {
    onDirty(Object.values(drafts).some(entry => Object.keys(entry.edits).length > 0) || Object.values(processDrafts).some(processingChanged));
  }, [drafts, processDrafts, onDirty]);
  useEffect(() => { setValidation(''); setResetConfirm(false); }, [asset?.id]);
  const entry = asset ? drafts[asset.id] : undefined;
  const value = entry?.value ?? (asset ? draftFor(asset) : undefined);
  const dirty = !!entry && Object.keys(entry.edits).length > 0;
  const settings = (asset && processDrafts[asset.id]?.value) || defaultProcessing();
  const processIds = selectedIds.length ? selectedIds : asset ? [asset.id] : [];
  const missingSequence = workspace.assets.find(item => processIds.includes(item.id) && item.sequence && workspace.assets.some(other => other.sequence === item.sequence && !processIds.includes(other.id)))?.sequence;
  const hasOperation = settings.snapToGrid || settings.hardenAlpha || settings.mapPalette || settings.trim || settings.scale !== 1;
  const operations = [settings.snapToGrid && '논리 픽셀 정렬', settings.hardenAlpha && '알파 이진화', settings.mapPalette && '팔레트 매핑', settings.trim && '여백 자르기', settings.scale !== 1 && `${settings.scale}× 확대`].filter(Boolean);
  const lastApplied = asset ? applied[asset.id] : undefined;
  const isApplied = !!lastApplied && JSON.stringify(lastApplied.settings) === JSON.stringify(settings) && JSON.stringify(lastApplied.ids) === JSON.stringify(processIds);

  const edit = <K extends keyof Draft>(key: K, nextValue: Draft[K]) => {
    if (!asset) return;
    setDrafts(previous => {
      const existing = previous[asset.id] ?? { value: draftFor(asset), edits: {} };
      const edits = { ...existing.edits, [key]: nextValue };
      if (nextValue === draftFor(asset)[key]) delete edits[key];
      return { ...previous, [asset.id]: { value: { ...draftFor(asset), ...edits }, edits } };
    });
    setValidation('');
    setSavedAsset('');
  };
  function updateProcessing(patch: Partial<ProcessSettings>) {
    if (!asset) return;
    setProcessDrafts(previous => {
      const existing = previous[asset.id] ?? { value: defaultProcessing(), saved: defaultProcessing() };
      return { ...previous, [asset.id]: { ...existing, value: { ...existing.value, ...patch } } };
    });
    setApplied(previous => { const next = { ...previous }; delete next[asset.id]; return next; });
  }
  async function save() {
    if (!asset || !value || !entry || !dirty) return;
    const edits = entry.edits;
    const patch: AssetPatch = {};
    if (edits.name !== undefined) {
      if (!/^[A-Za-z0-9][A-Za-z0-9_-]*$/.test(value.name)) { setValidation('파일명은 영문·숫자로 시작하고 영문, 숫자, 밑줄, 하이픈만 사용할 수 있습니다. .png는 제외해 주세요.'); return; }
      patch.name = value.name;
    }
    if (edits.frameIndex !== undefined) {
      if (!value.frameIndex.trim() || !Number.isInteger(Number(value.frameIndex)) || Number(value.frameIndex) < 0 || Number(value.frameIndex) > 1000000) { setValidation('프레임 번호는 0~1,000,000의 정수로 입력해 주세요.'); return; }
      patch.frameIndex = Number(value.frameIndex);
    }
    if (edits.durationMs !== undefined) {
      if (!value.durationMs.trim() || !Number.isInteger(Number(value.durationMs)) || Number(value.durationMs) < 1 || Number(value.durationMs) > 600000) { setValidation('프레임 시간은 1~600,000ms의 정수로 입력해 주세요.'); return; }
      patch.durationMs = Number(value.durationMs);
    }
    if (edits.layer !== undefined) {
      if (!value.layer.trim() || !Number.isInteger(Number(value.layer))) { setValidation('레이어는 정수로 입력해 주세요.'); return; }
      patch.layer = Number(value.layer);
    }
    if (edits.pivotX !== undefined) {
      if (!value.pivotX.trim() || !Number.isFinite(Number(value.pivotX))) { setValidation('피벗 X는 유효한 픽셀 좌표로 입력해 주세요.'); return; }
      patch.pivotX = Number(value.pivotX);
    }
    if (edits.pivotY !== undefined) {
      if (!value.pivotY.trim() || !Number.isFinite(Number(value.pivotY))) { setValidation('피벗 Y는 유효한 픽셀 좌표로 입력해 주세요.'); return; }
      patch.pivotY = Number(value.pivotY);
    }
    if (edits.profileId !== undefined) patch.profileId = value.profileId;
    if (edits.sequence !== undefined) patch.sequence = value.sequence;
    if (edits.notes !== undefined) patch.notes = value.notes;
    if (edits.reference !== undefined) patch.reference = value.reference;
    if (await run('리소스 정보 저장', () => api.updateAsset(asset.id, patch))) {
      setDrafts(previous => {
        const remaining = { ...previous[asset.id]?.edits };
        for (const key of Object.keys(edits) as (keyof Draft)[]) {
          if (remaining[key] === edits[key]) delete remaining[key];
        }
        return { ...previous, [asset.id]: { value: { ...value, ...remaining }, edits: remaining } };
      });
      setSavedAsset(asset.id);
    }
  }
  async function process() {
    if (!asset || !hasOperation || (settings.trim && missingSequence)) return;
    if (await run('선택 에셋 픽셀 정리', () => api.processAssets(processIds, settings))) {
      setProcessDrafts(previous => {
        const existing = previous[asset.id];
        return existing ? { ...previous, [asset.id]: { ...existing, saved: settings } } : previous;
      });
      setApplied(previous => ({ ...previous, [asset.id]: { settings: { ...settings }, ids: [...processIds] } }));
    }
  }

  return <section className="inspector workbench-inspector" aria-label={mode === 'resource' ? '리소스 정보' : '픽셀 정리'}>
    {!asset || !value ? <Empty title="선택한 에셋 없음"><p>목록에서 에셋을 선택하세요.</p></Empty> : <div className="inspector-body">
      {mode === 'resource' ? <>
        <div className="section-heading"><h2>리소스 정보</h2>{dirty && <span className="dirty-label">저장 전</span>}</div>
        <Field label="파일명" hint="영문·숫자·밑줄·하이픈 / .png 자동 추가"><input value={value.name} onChange={event => edit('name', event.target.value)} disabled={busy} spellCheck={false} /></Field>
        <Field label="원본 파일"><span className="read-value" title={asset.originalPath}>{asset.sourceName}</span></Field>
        <Choice label="제작 기준" value={value.profileId} onChange={next => edit('profileId', next)} disabled={busy} options={workspace.profiles.map(profile => ({ value: profile.id, label: profile.name }))} />
        <DetailPanel title="시퀀스 · 프레임 · 레이어" className="metadata-panel">
        <Field label="시퀀스" hint="같은 이름의 에셋을 프레임 순서대로 재생합니다."><input value={value.sequence} onChange={event => edit('sequence', event.target.value)} disabled={busy} /></Field>
        <div className="field-row">
          <Field label="프레임 번호"><input type="number" min="0" max="1000000" step="1" value={value.frameIndex} onChange={event => edit('frameIndex', event.target.value)} disabled={busy} /></Field>
          <Field label="레이어"><input type="number" step="1" value={value.layer} onChange={event => edit('layer', event.target.value)} disabled={busy} /></Field>
        </div>
        <Field label="프레임 시간 (ms)"><input type="number" min="1" max="600000" step="1" value={value.durationMs} onChange={event => edit('durationMs', event.target.value)} disabled={busy} /><input type="range" aria-label="프레임 시간 조절" min="16" max="1000" step="1" value={Math.min(1000, Math.max(16, Number(value.durationMs) || 16))} onChange={event => edit('durationMs', event.target.value)} disabled={busy} /></Field>
        </DetailPanel>
        <DetailPanel title="피벗 좌표" className="metadata-panel">
        <div className="field-row">
          <Field label="피벗 X (px)"><input type="number" step="any" value={value.pivotX} onChange={event => edit('pivotX', event.target.value)} disabled={busy} /></Field>
          <Field label="피벗 Y (px)"><input type="number" step="any" value={value.pivotY} onChange={event => edit('pivotY', event.target.value)} disabled={busy} /></Field>
        </div>
        <p className="helper compact">이미지 왼쪽 위가 (0, 0)입니다. 미리보기의 피벗 가이드로 위치를 확인하세요.</p>
        </DetailPanel>
        <Field label="에셋 메모"><textarea rows={4} value={value.notes} onChange={event => edit('notes', event.target.value)} disabled={busy} /></Field>
        <ToggleAction label="참고용 에셋" description="바탕 비교에 사용하며 승인·전달 대상에서 제외합니다." active={value.reference} onChange={next => edit('reference', next)} disabled={busy} />
        {validation && <p className="inline-error" role="alert">{validation}</p>}
        {savedAsset === asset.id && !dirty && <p className="tool-success" role="status"><Check size={14} />리소스 정보를 저장했습니다.</p>}
        <div className="form-actions">
          <button className="button primary" disabled={busy || !dirty} onClick={() => void save()}><Save size={15} />정보 저장</button>
          <button className="button" disabled={busy || !dirty} onClick={() => { setDrafts(previous => ({ ...previous, [asset.id]: { value: draftFor(asset), edits: {} } })); setValidation(''); }}>편집 취소</button>
        </div>
      </> : <>
        <div className="section-heading"><h2>픽셀 정리</h2></div>
        <div className="tool-operation-list">
          <ToggleAction label="논리 픽셀 정렬" description="제작 기준의 도트 격자에 맞춤" active={settings.snapToGrid} onChange={next => updateProcessing({ snapToGrid: next })} disabled={busy} />
          <ToggleAction label="알파 이진화" description="제작 기준의 임계값으로 투명·불투명 구분" active={settings.hardenAlpha} onChange={next => updateProcessing({ hardenAlpha: next })} disabled={busy} />
          <ToggleAction label="팔레트 매핑" description="각 에셋의 승인 팔레트로 색상 변경" active={settings.mapPalette} onChange={next => updateProcessing({ mapPalette: next })} disabled={busy} />
          <ToggleAction label="투명 여백 자르기" description="시퀀스 공통 경계로 자르고 피벗 보정" active={settings.trim} onChange={next => updateProcessing({ trim: next })} disabled={busy} />
        </div>
        <Choice label="출력 배율" value={settings.scale} onChange={scale => updateProcessing({ scale })} disabled={busy} options={[1, 2, 3, 4, 6, 8].map(scale => ({ value: scale, label: `${scale}×`, detail: scale === 1 ? '크기 유지' : '최근접 이웃 확대' }))} compact />
        <DetailPanel title="처리 방식과 주의사항">
          <p>선택한 처리만 보관 원본에서 다시 적용합니다. 현재 결과에 누적하지 않으며 원본 파일은 바꾸지 않습니다.</p>
          <dl className="tool-explanations">
            <dt>논리 픽셀 정렬</dt><dd>제작 기준의 픽셀 크기로 색을 정렬합니다.</dd>
            <dt>알파 이진화</dt><dd>제작 기준의 임계값으로 투명·불투명을 나눕니다.</dd>
            <dt>팔레트 매핑</dt><dd>각 에셋의 제작 기준에 지정된 가까운 색으로 바꿉니다.</dd>
            <dt>여백 자르기</dt><dd>같은 시퀀스의 모든 프레임이 필요합니다. 공통 경계로 잘라 프레임 사이 위치 차이를 유지하고 피벗을 보정합니다.</dd>
          </dl>
        </DetailPanel>
        {settings.trim && missingSequence && <p className="inline-error" role="alert">‘{missingSequence}’ 시퀀스의 모든 프레임을 선택해야 여백을 자를 수 있습니다.</p>}
        <div className="tool-action-footer">
          <div className="tool-action-summary"><strong>{selectedIds.length ? '선택한' : '현재'} 에셋 {processIds.length}개</strong><span>{operations.length ? operations.join(' · ') : '적용할 처리를 선택하세요.'}</span></div>
          <DetailPanel title="적용 대상 보기" className="tool-targets"><ul>{workspace.assets.filter(item => processIds.includes(item.id)).map(item => <li key={item.id}>{item.name}.png</li>)}</ul></DetailPanel>
          {hasOperation && <p className="tool-caution">보관 원본에서 다시 처리하며 디자인·QC 승인이 해제됩니다.{settings.snapToGrid && ' 디테일이 줄어들 수 있습니다.'}{settings.hardenAlpha && ' 부드러운 투명 효과가 사라질 수 있습니다.'}{settings.mapPalette && ' 원래 색을 잃을 수 있습니다.'}</p>}
          <button className="button primary full" disabled={busy || !processIds.length || !hasOperation || (settings.trim && !!missingSequence)} onClick={() => void process()}><SlidersHorizontal size={15} />{isApplied ? '같은 처리 다시 적용' : `${processIds.length}개에 적용`}</button>
          {isApplied && <p className="tool-success" role="status"><Check size={14} />{processIds.length}개에 적용했습니다. ‘나란히’ 보기에서 비교하세요.</p>}
          {processDrafts[asset.id] && processingChanged(processDrafts[asset.id]) && <button className="button tool-text-button" disabled={busy} onClick={() => setProcessDrafts(previous => ({ ...previous, [asset.id]: { ...previous[asset.id], value: previous[asset.id].saved } }))}>옵션 편집 취소</button>}
        </div>
        <DetailPanel title="현재 에셋 원본 복원" className="tool-reset">
          <p>{asset.name}.png의 처리 결과와 디자인·QC 승인을 초기화합니다. 보관 원본은 유지됩니다.</p>
          {resetConfirm ? <div className="confirm-inline"><p>현재 에셋을 원본으로 되돌릴까요?</p><div className="form-actions"><button className="button danger" disabled={busy} onClick={() => void run('원본 복원', () => api.resetAsset(asset.id)).then(ok => { if (ok) { setResetConfirm(false); setApplied(previous => { const next = { ...previous }; delete next[asset.id]; return next; }); } })}>원본 복원</button><button className="button" disabled={busy} onClick={() => setResetConfirm(false)}>취소</button></div></div> : <button className="button" disabled={busy || !asset.processed} onClick={() => setResetConfirm(true)}><RotateCcw size={14} />원본으로 되돌리기</button>}
        </DetailPanel>
      </>}
    </div>}
  </section>;
}
