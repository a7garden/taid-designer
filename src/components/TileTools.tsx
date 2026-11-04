import { useState } from 'react';
import type { Asset, TileSettings, Workspace } from '../domain';
import * as api from '../bridge';
import { Field } from './ui';
import type { WorkspaceAction } from './ui';

type Props = { workspace: Workspace; asset?: Asset; selectedIds: string[]; busy: boolean; run: WorkspaceAction; reportError: (message: string) => void };

export function TileTools({ workspace, asset, selectedIds, busy, run, reportError }: Props) {
  const [width, setWidth] = useState('32');
  const [height, setHeight] = useState('32');
  const [scale, setScale] = useState('1');
  const [mode, setMode] = useState<TileSettings['mode']>('pad');
  const [anchor, setAnchor] = useState<TileSettings['anchor']>('bottom-center');
  const [scope, setScope] = useState<'current' | 'selected'>('current');
  const ids = scope === 'selected' ? selectedIds : asset ? [asset.id] : [];
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
  const gridMismatch = asset && profile && (asset.width % profile.pixelSize !== 0 || asset.height % profile.pixelSize !== 0 || asset.metrics.gridViolations > 0);
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
    await run('현재 이미지에서 타일 규격 변형 만들기', () => api.tileAssets(ids, { width: w, height: h, scale: s, mode, anchor }));
  }

  return <section className="tile-tools" aria-label="타일 규격 맞추기">
    <h3>타일 규격 맞추기</h3>
    <p className="helper">너비·높이는 논리 도트 캔버스입니다. 현재 파일에서 새 미승인 변형과 전용 제작 기준을 만듭니다. 원본과 기존 제작 기준은 바꾸지 않습니다.</p>
    <Field label="변환할 에셋"><select value={scope} disabled={busy} onChange={event => setScope(event.target.value as typeof scope)}><option value="current">현재 이미지 {asset ? '1개' : '없음'}</option><option value="selected">선택한 이미지 {selectedIds.length}개</option></select></Field>
    {asset && profile && <p className="helper">현재 입력: {asset.width}×{asset.height}px ÷ {profile.pixelSize}px/도트 = {inputWidth}×{inputHeight}도트 · 「{profile.name}」</p>}
    {gridMismatch && <p className="inline-error">현재 입력의 도트 격자가 맞지 않습니다. 제작 기준의 도트 크기를 확인하고 격자 정리를 먼저 적용하세요. 타일 변환은 불균일한 도트를 자동으로 바꾸지 않습니다.</p>}
    <div className="tile-presets form-actions" aria-label="도트 캔버스 프리셋">{[16, 32, 64].map(size => <button className="button" key={size} disabled={busy} aria-pressed={w === size && h === size} onClick={() => { setWidth(String(size)); setHeight(String(size)); }}>{size}×{size}</button>)}</div>
    <div className="field-row">
      <Field label="캔버스 너비 (도트)"><input type="number" min={1} max={16384} step={1} value={width} disabled={busy} onChange={event => setWidth(event.target.value)} /></Field>
      <Field label="캔버스 높이 (도트)"><input type="number" min={1} max={16384} step={1} value={height} disabled={busy} onChange={event => setHeight(event.target.value)} /></Field>
    </div>
    <Field label="출력 도트 배율 (px/도트)"><input type="number" min={1} max={64} step={1} value={scale} disabled={busy} onChange={event => setScale(event.target.value)} /></Field>
    <div className="field-row">
      <Field label="크기 조정"><select value={mode} disabled={busy} onChange={event => setMode(event.target.value as TileSettings['mode'])}><option value="pad">여백 추가 · 그림 크기 유지</option><option value="fit">맞춤 · 확대 또는 축소</option></select></Field>
      <Field label="전체 캔버스 정렬"><select value={anchor} disabled={busy} onChange={event => setAnchor(event.target.value as TileSettings['anchor'])}><option value="bottom-center">하단 중앙</option><option value="center">중앙</option></select></Field>
    </div>
    <p className="tile-formula" aria-live="polite">{valid ? `${w}×${h}도트 × ${s}px/도트 → ${w * s}×${h * s}px PNG` : '양의 정수와 지원되는 출력 크기를 입력하세요.'}</p>
    {placed && <p className="helper">현재 그림 전체 캔버스 배치: {placed[0]}×{placed[1]}도트 · 왼쪽 여백 {Math.floor((w - placed[0]) / 2)}도트 · 위쪽 여백 {anchor === 'center' ? Math.floor((h - placed[1]) / 2) : h - placed[1]}도트</p>}
    {mode === 'fit' ? <p className={oversized ? 'inline-error' : 'helper'}>{oversized ? '축소가 필요합니다. 최근접 도트 선택으로 색은 늘어나지 않지만, 한 도트 세부나 윤곽이 사라질 수 있습니다.' : '그림이 작으면 들어가는 최대 정수 배율로 확대합니다. 남는 공간은 투명 여백입니다.'}</p> : oversized && <p className="inline-error">선택한 그림이 캔버스보다 큽니다. 여백 모드는 잘라내지 않으므로 더 큰 크기 또는 맞춤 모드를 선택하세요.</p>}
    <p className="helper">보이는 영역만 재중앙화하지 않습니다. 전체 캔버스 기준 정렬과 실제 크기 변화에 따라 피벗을 변환하고, 프레임 시간·레이어·참고용 상태를 유지합니다. 각 결과의 새 제작 기준은 팔레트·알파 정책·단계를 상속합니다.</p>
    <button className="button primary" disabled={busy || !ids.length || !valid || (mode === 'pad' && oversized)} onClick={() => void create()}>{ids.length}개 타일 변형 만들기</button>
  </section>;
}
