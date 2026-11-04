import { useEffect, useState } from 'react';
import { Check, FileUp, Pencil } from 'lucide-react';
import type { Asset, PaletteReport, Workspace } from '../domain';
import * as api from '../bridge';
import { errorMessage, Field, formatNumber } from './ui';
import type { WorkspaceAction } from './ui';
import '../workbench-tools.css';

type Props = { workspace: Workspace; asset?: Asset; selectedIds: string[]; busy: boolean; run: WorkspaceAction; reportError: (message: string) => void; onDirty?: (dirty: boolean) => void };

function parseDraft(text: string): string[] {
  const values = [...new Set(text.split(/[\s,;]+/).filter(Boolean).map(value => `#${value.replace(/^#/, '').toUpperCase()}`))];
  if (values.some(value => !/^#[0-9A-F]{6}$/.test(value))) throw new Error('색상은 #RRGGBB로 입력하고 줄바꿈 또는 쉼표로 구분해 주세요.');
  if (values.length > 256) throw new Error('승인 팔레트는 서로 다른 RGB 색상을 256개까지 지정할 수 있습니다.');
  return values;
}
type PaletteDraft = { text: string; touched: boolean };

function hasPaletteEdits(entry: PaletteDraft, palette: string[]): boolean {
  if (!entry.touched) return false;
  try { return JSON.stringify(parseDraft(entry.text)) !== JSON.stringify(palette); }
  catch { return true; }
}

export function PaletteTools({ workspace, asset, selectedIds, busy, run, reportError, onDirty }: Props) {
  const [result, setResult] = useState<{ key: string; report: PaletteReport }>();
  const [loading, setLoading] = useState(false);
  const [readError, setReadError] = useState('');
  const [refresh, setRefresh] = useState(0);
  const [profileId, setProfileId] = useState('');
  const [drafts, setDrafts] = useState<Record<string, PaletteDraft>>({});
  const [editorOpen, setEditorOpen] = useState(false);
  const [savedProfile, setSavedProfile] = useState('');
  const [mapped, setMapped] = useState('');
  const [validation, setValidation] = useState('');
  const [fileLoading, setFileLoading] = useState(false);
  const [scope, setScope] = useState<'current' | 'selected'>('current');
  const profile = workspace.profiles.find(item => item.id === asset?.profileId);
  const key = asset ? JSON.stringify([workspace.root, asset.id, asset.currentHash, profile?.palette]) : '';
  const report = result?.key === key ? result.report : undefined;
  const targetProfile = workspace.profiles.find(item => item.id === profileId);
  const draft = (drafts[profileId]?.touched ? drafts[profileId].text : targetProfile?.palette.join('\n')) ?? '';
  const targetCount = workspace.assets.filter(item => item.profileId === profileId).length;
  const ids = scope === 'selected' ? selectedIds : asset ? [asset.id] : [];
  const missingPalette = workspace.assets.find(item => ids.includes(item.id) && !workspace.profiles.find(rule => rule.id === item.profileId)?.palette.length);
  let draftColors: string[] = [];
  let draftValid = true;
  try { draftColors = parseDraft(draft); } catch { draftValid = false; }
  const draftDirty = !!drafts[profileId] && hasPaletteEdits(drafts[profileId], targetProfile?.palette ?? []);
  const anyDraftDirty = Object.entries(drafts).some(([id, entry]) => hasPaletteEdits(entry, workspace.profiles.find(item => item.id === id)?.palette ?? []));
  useEffect(() => { onDirty?.(anyDraftDirty); }, [anyDraftDirty, onDirty]);
  useEffect(() => { setProfileId(asset?.profileId ?? ''); setValidation(''); }, [workspace.root, asset?.id, asset?.profileId]);

  function editDraft(text: string) {
    if (!profileId) return;
    setDrafts(previous => ({ ...previous, [profileId]: { text, touched: true } }));
    setValidation('');
    setSavedProfile('');
    setMapped('');
    setEditorOpen(true);
  }

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
      if (path) editDraft((await api.readPalette(path)).join('\n'));
    } catch (error) { reportError(errorMessage(error)); }
    finally { setFileLoading(false); }
  }
  async function savePalette() {
    if (!targetProfile) return;
    try {
      const palette = parseDraft(draft);
      setValidation('');
      if (await run('승인 팔레트 저장 및 연결 에셋 재검사', () => api.saveProfile({ ...targetProfile, palette }))) {
        setDrafts(previous => previous[profileId]?.text === draft ? { ...previous, [profileId]: { text: draft, touched: false } } : previous);
        setSavedProfile(profileId);
        setEditorOpen(false);
      }
    } catch (error) { setValidation(errorMessage(error)); }
  }

  return <section className="palette-tools workbench-tool" aria-label="팔레트 검사와 변환">
    <div className="section-heading"><h3>이미지 색상</h3><button className="button tool-text-button" disabled={busy || loading || !asset} onClick={() => setRefresh(value => value + 1)}>다시 집계</button></div>
    {!asset ? <p className="helper">검사할 이미지를 선택하세요.</p> : <>
      {loading && <p className="tool-inline-note" role="status">색상 집계 중…</p>}
      {readError && <p className="inline-error" role="alert">{readError}</p>}
      {report && <>
        <p className="palette-count"><strong>{formatNumber(report.colorCount)}색</strong><span>보이는 픽셀 {formatNumber(report.visiblePixels)}개</span></p>
        <div className="palette-swatches actual-palette-swatches" aria-label="현재 이미지에서 많이 쓰인 RGB 색상 최대 32개">
          {report.dominantColors.map(color => <span key={color.hex} role="img" aria-label={`${color.hex}, ${formatNumber(color.pixels)}픽셀, ${(color.frequency * 100).toFixed(1)}%`} title={`${color.hex} · ${formatNumber(color.pixels)}px · ${(color.frequency * 100).toFixed(1)}%`} style={{ backgroundColor: color.hex }} />)}
        </div>
        <p className={report.approvedPaletteSize && report.outsidePixels ? 'inline-error' : 'tool-inline-note'}>
          {!report.approvedPaletteSize ? '승인 팔레트 미지정 · 적합 여부 미판정' : report.outsidePixels ? `승인 팔레트 밖 ${formatNumber(report.outsideColors)}색 · ${formatNumber(report.outsidePixels)}픽셀` : `승인 팔레트 ${report.approvedPaletteSize}색 안에 있습니다.`}
        </p>
        <details className="tool-disclosure">
          <summary>RGB별 사용량 · 최대 32색</summary>
          <p>완전 투명 픽셀은 제외하고 반투명 픽셀은 RGB별로 셉니다. 현재 파일만 읽습니다.</p>
          <div className="palette-histogram">{report.dominantColors.map(color => <div className="palette-color-row" key={color.hex}>
            <span className="palette-color-chip" style={{ backgroundColor: color.hex }} aria-hidden="true" /><code>{color.hex}</code><span>{formatNumber(color.pixels)}px</span><span>{(color.frequency * 100).toFixed(1)}%</span>
          </div>)}</div>
        </details>
        {report.outsideSamples.length > 0 && <details className="tool-disclosure"><summary>팔레트 밖 색상과 가까운 승인 RGB</summary>
          <p>최대 24색의 RGB 제곱 거리 제안입니다. 형태나 명암의 적합성을 대신 판단하지 않습니다.</p>
          {report.outsideSamples.map(color => <div className="palette-color-row" key={color.hex}>
            <span className="palette-color-chip" style={{ backgroundColor: color.hex }} aria-hidden="true" /><code>{color.hex}</code><span>{formatNumber(color.pixels)}px → {color.nearestApproved}</span><span>거리² {color.distanceSquared}</span>
          </div>)}
        </details>}
      </>}
    </>}

    <section className="palette-profile-section">
      <div className="section-heading"><h3>승인 팔레트</h3>{draftDirty && <span className="dirty-label">저장 전</span>}</div>
      <Field label="저장할 제작 기준"><select value={profileId} disabled={busy || fileLoading} onChange={event => { setProfileId(event.target.value); setValidation(''); setSavedProfile(''); }}>
        <option value="">제작 기준 선택</option>{workspace.profiles.map(item => <option key={item.id} value={item.id}>{item.name}{item.id === asset?.profileId ? ' · 현재 이미지' : ''}{drafts[item.id] && hasPaletteEdits(drafts[item.id], item.palette) ? ' · 저장 전' : ''}</option>)}
      </select></Field>
      {targetProfile && <>
        <div className="palette-profile-summary"><span>{targetProfile.palette.length ? `저장된 색상 ${targetProfile.palette.length}개` : '색상 제한 없음'}</span><button className="button tool-text-button" disabled={busy} aria-expanded={editorOpen} onClick={() => setEditorOpen(value => !value)}><Pencil size={13} />{editorOpen ? '편집 접기' : '편집'}</button></div>
        {targetProfile.palette.length > 0 && <div className="palette-swatches approved-palette-swatches" aria-label={`저장된 승인 팔레트 ${targetProfile.palette.length}색`}>{targetProfile.palette.map(color => <span key={color} role="img" aria-label={color} title={color} style={{ backgroundColor: color }} />)}</div>}
      </>}
      <details className="tool-disclosure palette-editor" open={editorOpen} onToggle={event => setEditorOpen(event.currentTarget.open)}>
        <summary>팔레트 가져오기 · 붙여넣기{draftDirty ? ' · 저장 전' : ''}</summary>
        <div className="palette-tool-actions form-actions">
          <button className="button" disabled={busy || fileLoading || !targetProfile} onClick={() => void importFile()}><FileUp size={14} />파일 가져오기</button>
          <button className="button" disabled={busy || !targetProfile || !report?.allColors?.length} onClick={() => { if (report?.allColors) editDraft(report.allColors.map(color => color.hex).join('\n')); }}>현재 이미지 색 사용</button>
        </div>
        {report && report.allColors === null && <p className="tool-inline-note">이미지가 256색을 초과해 전체 색을 가져올 수 없습니다.</p>}
        <Field label="RGB 색상 붙여넣기" hint="#RRGGBB · 줄바꿈 또는 쉼표로 구분 · 최대 256색">
          <textarea className="mono" rows={5} maxLength={8192} value={draft} spellCheck={false} disabled={busy || fileLoading || !targetProfile} onChange={event => editDraft(event.target.value)} />
        </Field>
        {draftValid && <><p className="tool-inline-note">초안 {draftColors.length}색{!draftColors.length && ' · 저장하면 팔레트 제한 해제'}</p><div className="palette-swatches draft-palette-swatches" aria-label={`입력 팔레트 ${draftColors.length}색`}>{draftColors.map(color => <span key={color} role="img" aria-label={color} title={color} style={{ backgroundColor: color }} />)}</div></>}
        {validation && <p className="inline-error" role="alert">{validation}</p>}
        {targetProfile && <p className="tool-caution">「{targetProfile.name}」의 연결 에셋 {targetCount}개를 재검사하고 승인을 해제합니다. 이미지 픽셀은 바꾸지 않습니다.{draftValid && !draftColors.length && ' 빈 팔레트를 저장하면 색상 제한이 해제됩니다.'}</p>}
        <div className="form-actions"><button className="button" disabled={busy || fileLoading || !targetProfile || !draftDirty} onClick={() => void savePalette()}>팔레트 저장</button>
          <button className="button tool-text-button" disabled={busy || fileLoading || !draftDirty} onClick={() => { setDrafts(previous => { const next = { ...previous }; delete next[profileId]; return next; }); setValidation(''); setEditorOpen(false); }}>편집 취소</button></div>
      </details>
      {savedProfile && savedProfile === profileId && !draftDirty && <p className="tool-success" role="status"><Check size={14} />팔레트를 저장하고 연결 에셋을 재검사했습니다.</p>}
    </section>

    <section className="palette-map-settings" hidden={editorOpen}>
      <h3>팔레트로 색상 맞춤</h3>
      <Field label="변환 대상"><select value={scope} disabled={busy} onChange={event => setScope(event.target.value as typeof scope)}><option value="current">현재 이미지 {asset ? '1개' : '없음'}</option><option value="selected">선택한 이미지 {selectedIds.length}개</option></select></Field>
    </section>
    <div className="tool-action-footer" hidden={editorOpen}>
      <div className="tool-action-summary"><strong>{scope === 'selected' ? '선택한' : '현재'} 이미지 {ids.length}개 → 새 미승인 변형 {ids.length}개</strong><span>각 에셋에 저장된 팔레트 사용 · 원본과 이전 처리 결과 유지</span></div>
      <p className="tool-caution">RGB 색을 바꾸고 투명도(알파)는 유지합니다. 위의 미저장 초안은 사용하지 않습니다.</p>
      {missingPalette && <p className="inline-error">「{missingPalette.name}」의 제작 기준에 승인 팔레트를 먼저 저장하세요.</p>}
      <button className="button primary full" disabled={busy || !ids.length || !!missingPalette} onClick={() => void run('현재 이미지에서 팔레트 변형 만들기', () => api.mapPaletteAssets(ids)).then(ok => { if (ok) setMapped(`${ids.length}개 팔레트 변형 생성`); })}>{ids.length}개 팔레트 변형 만들기</button>
      {mapped && <p className="tool-success" role="status"><Check size={14} />직전 작업: {mapped}</p>}
    </div>
  </section>;
}
