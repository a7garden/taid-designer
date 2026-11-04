import { useEffect, useState } from 'react';
import { Check } from 'lucide-react';
import type { Asset, TileSettings, Workspace } from '../domain';
import * as api from '../bridge';
import { Field } from './ui';
import type { WorkspaceAction } from './ui';
import '../workbench-tools.css';

type Props = { workspace: Workspace; asset?: Asset; selectedIds: string[]; busy: boolean; run: WorkspaceAction; reportError: (message: string) => void; onDirty?: (dirty: boolean) => void };

export function TileTools({ workspace, asset, selectedIds, busy, run, reportError, onDirty }: Props) {
  const [width, setWidth] = useState('32');
  const [height, setHeight] = useState('32');
  const [scale, setScale] = useState('1');
  const [mode, setMode] = useState<TileSettings['mode']>('pad');
  const [anchor, setAnchor] = useState<TileSettings['anchor']>('bottom-center');
  const [scope, setScope] = useState<'current' | 'selected'>('current');
  const configuration = JSON.stringify([width, height, scale, mode, anchor, scope]);
  const [savedConfiguration, setSavedConfiguration] = useState(configuration);
  const [created, setCreated] = useState<{ configuration: string; ids: string[] }>();
  useEffect(() => { onDirty?.(configuration !== savedConfiguration); }, [configuration, savedConfiguration, onDirty]);
  const ids = scope === 'selected' ? selectedIds : asset ? [asset.id] : [];
  const isCreated = created?.configuration === configuration;
  const profile = workspace.profiles.find(item => item.id === asset?.profileId);
  const inputWidth = asset && profile ? asset.width / profile.pixelSize : undefined;
  const inputHeight = asset && profile ? asset.height / profile.pixelSize : undefined;
  const w = Number(width), h = Number(height), s = Number(scale);
  const valid = [w, h, s].every(value => Number.isInteger(value) && value > 0) && s <= 64 && w * s <= 16384 && h * s <= 16384 && w * h * s * s <= 16 * 1024 * 1024;
  const selected = workspace.assets.filter(item => ids.includes(item.id));
  const oversized = valid && selected.some(item => {
    const rule = workspace.profiles.find(candidate => candidate.id === item.profileId);
    return rule && (item.width / rule.pixelSize > w || item.height / rule.pixelSize > h);
  });
  const gridMismatch = selected.some(item => {
    const rule = workspace.profiles.find(candidate => candidate.id === item.profileId);
    return !rule || item.width % rule.pixelSize !== 0 || item.height % rule.pixelSize !== 0 || item.metrics.gridViolations > 0;
  });
  let placed: [number, number] | undefined;
  if (valid && inputWidth && inputHeight && Number.isInteger(inputWidth) && Number.isInteger(inputHeight)) {
    if (mode === 'pad') {
      if (inputWidth <= w && inputHeight <= h) placed = [inputWidth, inputHeight];
    } else if (inputWidth <= w && inputHeight <= h) {
      const factor = Math.floor(Math.min(w / inputWidth, h / inputHeight));
      placed = [inputWidth * factor, inputHeight * factor];
    } else if (w * inputHeight <= h * inputWidth) {
      placed = [w, Math.max(1, Math.round(inputHeight * w / inputWidth))];
    } else {
      placed = [Math.max(1, Math.round(inputWidth * h / inputHeight)), h];
    }
  }
  async function create() {
    if (!valid) { reportError('도트 너비·높이는 양의 정수, 출력 배율은 1~64로 입력하세요. 출력은 한 변 16,384px, 총 16메가픽셀 이하입니다.'); return; }
    if (!ids.length) return;
    if (await run('현재 이미지에서 타일 규격 변형 만들기', () => api.tileAssets(ids, { width: w, height: h, scale: s, mode, anchor }))) {
      setSavedConfiguration(configuration);
      setCreated({ configuration, ids: [...ids] });
    }
  }

  return <section className="tile-tools workbench-tool" aria-label="타일 규격 맞추기">
    <div className="section-heading"><h3>크기 맞춤</h3></div>
    <Field label="변환 대상"><select value={scope} disabled={busy} onChange={event => setScope(event.target.value as typeof scope)}><option value="current">현재 이미지 {asset ? '1개' : '없음'}</option><option value="selected">선택한 이미지 {selectedIds.length}개</option></select></Field>
    <fieldset className="tool-fieldset">
      <legend>논리 캔버스</legend>
      <div className="tile-presets tool-segmented" aria-label="도트 캔버스 프리셋">{[32, 64, 128].map(size => <button className="button" key={size} disabled={busy} aria-pressed={w === size && h === size} onClick={() => { setWidth(String(size)); setHeight(String(size)); }}>{size} × {size}</button>)}</div>
      <div className="field-row">
        <Field label="너비 (도트)"><input type="number" min={1} max={16384} step={1} value={width} disabled={busy} onChange={event => setWidth(event.target.value)} /></Field>
        <Field label="높이 (도트)"><input type="number" min={1} max={16384} step={1} value={height} disabled={busy} onChange={event => setHeight(event.target.value)} /></Field>
      </div>
    </fieldset>
    <fieldset className="tool-fieldset">
      <legend>그림 배치</legend>
      <div className="tool-segmented" aria-label="크기 조정 방식">
        <button className="button" aria-pressed={mode === 'pad'} disabled={busy} onClick={() => setMode('pad')}>크기 유지 · 여백</button>
        <button className="button" aria-pressed={mode === 'fit'} disabled={busy} onClick={() => setMode('fit')}>캔버스에 맞춤</button>
      </div>
    </fieldset>
    <Field label="출력 배율 (px/도트)"><input type="number" min={1} max={64} step={1} value={scale} disabled={busy} onChange={event => setScale(event.target.value)} /></Field>
    <div className="tool-dimensions" aria-live="polite">
      <span>출력 PNG</span><strong>{valid ? `${w * s} × ${h * s} px` : '출력 크기 확인 필요'}</strong>
      <span>{valid ? `${w} × ${h}도트 · ${s}px/도트` : '한 변 16,384px · 총 16메가픽셀 이하'}</span>
    </div>
    {asset && profile && <p className="tool-inline-note">현재 입력 {inputWidth} × {inputHeight}도트 <span>({asset.width} × {asset.height}px ÷ {profile.pixelSize}px/도트)</span></p>}
    {gridMismatch && <p className="inline-error">변환 대상의 도트 격자가 맞지 않습니다. 제작 기준의 도트 크기를 확인하고 격자 정리를 먼저 적용하세요.</p>}
    <details className="tool-disclosure">
      <summary>정렬과 변환 세부사항</summary>
      <Field label="전체 캔버스 정렬"><select value={anchor} disabled={busy} onChange={event => setAnchor(event.target.value as TileSettings['anchor'])}><option value="bottom-center">하단 중앙</option><option value="center">중앙</option></select></Field>
      {placed && <p>현재 그림 배치 {placed[0]} × {placed[1]}도트 · 왼쪽 여백 {Math.floor((w - placed[0]) / 2)}도트 · 위쪽 여백 {anchor === 'center' ? Math.floor((h - placed[1]) / 2) : h - placed[1]}도트</p>}
      <p>맞춤 모드에서 작은 그림은 들어가는 최대 정수 배율로 확대하고 남는 공간은 투명 여백으로 채웁니다.</p>
      <p>보이는 영역만 재중앙화하지 않습니다. 전체 캔버스를 기준으로 정렬하고 크기 변화에 맞춰 피벗을 변환합니다. 프레임 시간·레이어·참고용 상태와 제작 기준의 팔레트·알파 정책·단계를 유지합니다.</p>
    </details>
    <div className="tool-action-footer">
      <div className="tool-action-summary"><strong>{scope === 'selected' ? '선택한' : '현재'} 이미지 {ids.length}개 → 새 변형 {ids.length}개</strong><span>새 미승인 PNG와 전용 제작 기준 · 원본과 기존 기준 유지</span></div>
      {mode === 'fit' && oversized && <p className="tool-caution">최근접 도트 선택으로 축소합니다. 색은 늘어나지 않지만 한 도트 세부나 윤곽이 사라질 수 있습니다.</p>}
      {mode === 'pad' && oversized && <p className="inline-error">그림이 캔버스보다 큽니다. 더 큰 크기 또는 맞춤 모드를 선택하세요. 여백 모드는 그림을 자르지 않습니다.</p>}
      <button className="button primary full" disabled={busy || !ids.length || !valid || gridMismatch || (mode === 'pad' && oversized)} onClick={() => void create()}>{ids.length}개 크기 변형 만들기</button>
      {isCreated && <p className="tool-success" role="status"><Check size={14} />직전 작업: 크기 변형 {created.ids.length}개 생성</p>}
    </div>
  </section>;
}
