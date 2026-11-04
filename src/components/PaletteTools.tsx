import { useEffect, useState } from 'react';
import type { Asset, PaletteReport, Workspace } from '../domain';
import * as api from '../bridge';
import { errorMessage, Field, formatNumber } from './ui';
import type { WorkspaceAction } from './ui';

type Props = { workspace: Workspace; asset?: Asset; selectedIds: string[]; busy: boolean; run: WorkspaceAction; reportError: (message: string) => void; onDirty?: (dirty: boolean) => void };

function parseDraft(text: string): string[] {
  const values = [...new Set(text.split(/[\s,;]+/).filter(Boolean).map(value => `#${value.replace(/^#/, '').toUpperCase()}`))];
  if (values.some(value => !/^#[0-9A-F]{6}$/.test(value))) throw new Error('색상은 #RRGGBB로 입력하고 줄바꿈 또는 쉼표로 구분해 주세요.');
  if (values.length > 256) throw new Error('승인 팔레트는 서로 다른 RGB 색상을 256개까지 지정할 수 있습니다.');
  return values;
}

export function PaletteTools({ workspace, asset, selectedIds, busy, run, reportError, onDirty }: Props) {
  const [result, setResult] = useState<{ key: string; report: PaletteReport }>();
  const [loading, setLoading] = useState(false);
  const [readError, setReadError] = useState('');
  const [refresh, setRefresh] = useState(0);
  const [profileId, setProfileId] = useState('');
  const [draft, setDraft] = useState('');
  const [touched, setTouched] = useState(false);
  const [validation, setValidation] = useState('');
  const [fileLoading, setFileLoading] = useState(false);
  const [scope, setScope] = useState<'current' | 'selected'>('current');
  const profile = workspace.profiles.find(item => item.id === asset?.profileId);
  const key = asset ? JSON.stringify([workspace.root, asset.id, asset.currentHash, profile?.palette]) : '';
  const report = result?.key === key ? result.report : undefined;
  const targetProfile = workspace.profiles.find(item => item.id === profileId);
  const targetCount = workspace.assets.filter(item => item.profileId === profileId).length;
  const ids = scope === 'selected' ? selectedIds : asset ? [asset.id] : [];
  const missingPalette = workspace.assets.find(item => ids.includes(item.id) && !workspace.profiles.find(rule => rule.id === item.profileId)?.palette.length);
  let draftColors: string[] = [];
  let draftValid = true;
  try { draftColors = parseDraft(draft); } catch { draftValid = false; }
  const draftDirty = touched && (!draftValid || !targetProfile || JSON.stringify(draftColors) !== JSON.stringify(targetProfile.palette));
  useEffect(() => { onDirty?.(draftDirty); }, [draftDirty, onDirty]);

  useEffect(() => {
    if (!asset || busy) return;
    let cancelled = false;
    setLoading(true);
    setResult(undefined);
    setReadError('');
    api.inspectPalette(asset.id).then(next => {
      if (!cancelled) setResult({ key, report: next });
    }).catch(error => {
      if (!cancelled) setReadError(errorMessage(error));
    }).finally(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, [asset?.id, key, busy, refresh]);

  async function importFile() {
    setFileLoading(true);
    try {
      const path = await api.choosePalette();
      if (path) { setDraft((await api.readPalette(path)).join('\n')); setTouched(true); setValidation(''); }
    } catch (error) { reportError(errorMessage(error)); }
    finally { setFileLoading(false); }
  }
  async function savePalette() {
    if (!targetProfile) return;
    try {
      const palette = parseDraft(draft);
      setValidation('');
      if (await run('승인 팔레트 저장 및 연결 에셋 재검사', () => api.saveProfile({ ...targetProfile, palette }))) setTouched(false);
    } catch (error) { setValidation(errorMessage(error)); }
  }

  return <section className="palette-tools" aria-label="팔레트 검사와 변환">
    <h3>현재 이미지의 실제 색상</h3>
    {!asset ? <p className="helper">검사할 이미지를 선택하세요.</p> : <>
      <p className="helper">완전 투명 픽셀은 제외하고, 반투명 픽셀은 RGB별로 셉니다. 현재 파일만 읽으며 원본과 제작 기준은 바꾸지 않습니다.</p>
      {loading && <p role="status">색상 집계 중…</p>}
      {readError && <p className="inline-error" role="alert">{readError}</p>}
      {report && <>
        <p><strong>{formatNumber(report.colorCount)}색</strong> · 보이는 픽셀 {formatNumber(report.visiblePixels)}개</p>
        <p className={report.approvedPaletteSize && report.outsidePixels ? 'inline-error' : 'helper'}>
          {!report.approvedPaletteSize ? '승인 팔레트 미지정 · 색상 적합 여부는 판정하지 않습니다.' : report.outsidePixels ? `승인 팔레트 밖 ${formatNumber(report.outsideColors)}색 · ${formatNumber(report.outsidePixels)}픽셀` : `승인 팔레트 ${report.approvedPaletteSize}색 안에 있습니다.`}
        </p>
        <div className="palette-histogram" aria-label="많이 쓰인 RGB 색상 최대 32개">
          {report.dominantColors.map(color => <div className="palette-color-row" key={color.hex}>
            <span className="palette-color-chip" style={{ backgroundColor: color.hex }} aria-hidden="true" />
            <code>{color.hex}</code><span>{formatNumber(color.pixels)}px</span><span>{(color.frequency * 100).toFixed(1)}%</span>
          </div>)}
        </div>
        {report.outsideSamples.length > 0 && <details className="rule-details"><summary>팔레트 밖 색상 · 가까운 승인 RGB (최대 24개)</summary>
          <p className="helper">RGB 제곱 거리 제안이며, 형태나 명암의 적합성을 대신 판단하지 않습니다.</p>
          {report.outsideSamples.map(color => <div className="palette-color-row" key={color.hex}>
            <span className="palette-color-chip" style={{ backgroundColor: color.hex }} aria-hidden="true" /><code>{color.hex}</code><span>{formatNumber(color.pixels)}px → {color.nearestApproved}</span><span>거리² {color.distanceSquared}</span>
          </div>)}
        </details>}
      </>}
      <button className="button" disabled={busy || loading} onClick={() => setRefresh(value => value + 1)}>색상 다시 집계</button>
    </>}

    <div className="form-divider" />
    <h3>승인 팔레트 지정</h3>
    <Field label="저장할 제작 기준"><select value={profileId} disabled={busy} onChange={event => { setProfileId(event.target.value); setValidation(''); }}>
      <option value="">제작 기준 선택</option>{workspace.profiles.map(item => <option key={item.id} value={item.id}>{item.name}</option>)}
    </select></Field>
    <div className="palette-tool-actions form-actions">
      <button className="button" disabled={busy || fileLoading} onClick={() => void importFile()}>.hex / .gpl 파일 읽기</button>
      <button className="button" disabled={busy || !targetProfile} onClick={() => { setDraft(targetProfile!.palette.join('\n')); setTouched(false); setValidation(''); }}>저장된 팔레트 불러오기</button>
      <button className="button" disabled={busy || !report?.allColors?.length} onClick={() => { if (report?.allColors) { setDraft(report.allColors.map(color => color.hex).join('\n')); setTouched(true); setValidation(''); } }}>현재 이미지 색을 초안으로</button>
    </div>
    {report && report.allColors === null && <p className="helper">현재 이미지는 256색을 초과하여 전체 색을 승인 팔레트로 가져오지 않습니다.</p>}
    <Field label="승인 RGB 색상 붙여넣기" hint="#RRGGBB · 최대 256색. 빈칸을 저장하면 팔레트 제한을 해제합니다.">
      <textarea className="mono" rows={5} maxLength={8192} value={draft} spellCheck={false} disabled={busy || fileLoading} onChange={event => { setDraft(event.target.value); setTouched(true); setValidation(''); }} />
    </Field>
    <div className="palette-swatches" aria-label={`입력 팔레트 ${draftColors.length}색`}>{draftColors.map(color => <span key={color} title={color} style={{ backgroundColor: color }} />)}</div>
    {targetProfile && <p className="helper">「{targetProfile.name}」에 연결된 {targetCount}개 에셋에 적용합니다. 기준이 바뀌면 해당 에셋의 승인이 무효화됩니다. 이미지 색은 아직 변환하지 않습니다.</p>}
    {validation && <p className="inline-error" role="alert">{validation}</p>}
    <button className="button primary" disabled={busy || fileLoading || !targetProfile} onClick={() => void savePalette()}>이 제작 기준에 팔레트 저장</button>

    <div className="form-divider" />
    <h3>승인 팔레트로 색상 변환</h3>
    <Field label="변환할 에셋"><select value={scope} disabled={busy} onChange={event => setScope(event.target.value as typeof scope)}><option value="current">현재 이미지 {asset ? '1개' : '없음'}</option><option value="selected">선택한 이미지 {selectedIds.length}개</option></select></Field>
    <p className="helper">각 에셋의 저장된 팔레트를 사용합니다. 현재 이미지의 투명도를 유지한 새 미승인 변형을 만들며, 이전 누끼·정리 결과와 원본을 보존합니다.</p>
    {missingPalette && <p className="inline-error">「{missingPalette.name}」의 제작 기준에 승인 팔레트를 먼저 저장하세요.</p>}
    <button className="button" disabled={busy || !ids.length || !!missingPalette} onClick={() => void run('현재 이미지에서 팔레트 변형 만들기', () => api.mapPaletteAssets(ids))}>{ids.length}개 팔레트 변형 만들기</button>
  </section>;
}
