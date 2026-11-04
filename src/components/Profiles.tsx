import { useEffect, useState } from 'react';
import { Copy, FileText, Plus, Save } from 'lucide-react';
import type { RuleProfile, Workspace } from '../domain';
import * as api from '../bridge';
import { buildPrompt, Field, useModalKeyboard } from './ui';
import type { RunAction, WorkspaceAction } from './ui';

type ProfileDraft = Omit<RuleProfile, 'pixelSize' | 'width' | 'height' | 'alphaThreshold' | 'palette'> & { pixelSize: string; width: string; height: string; alphaThreshold: string; palette: string };
type Entry = { value: ProfileDraft; dirty: boolean };
const draftFor = (p: RuleProfile): ProfileDraft => ({ ...p, pixelSize: String(p.pixelSize), width: p.width === null ? '' : String(p.width), height: p.height === null ? '' : String(p.height), alphaThreshold: String(p.alphaThreshold), palette: p.palette.join('\n') });
export function Profiles({ workspace, busy, run, runAction, onDirty, onSelectAsset }: { workspace: Workspace; busy: boolean; run: WorkspaceAction; runAction: RunAction; onDirty: (dirty: boolean) => void; onSelectAsset: (id: string) => void }) {
  const [selectedId, setSelectedId] = useState(workspace.profiles[0]?.id ?? '');
  const [drafts, setDrafts] = useState<Record<string, Entry>>({});
  const [notes, setNotes] = useState(workspace.notes);
  const [notesDirty, setNotesDirty] = useState(false);
  const [validation, setValidation] = useState('');
  const [prompt, setPrompt] = useState('');
  useModalKeyboard(!!prompt, () => setPrompt(''));
  useEffect(() => { setDrafts(old => { const next = { ...old }; for (const p of workspace.profiles) if (!next[p.id]?.dirty) next[p.id] = { value: draftFor(p), dirty: false }; return next; }); }, [workspace.profiles]);
  useEffect(() => { if (!notesDirty) setNotes(workspace.notes); }, [workspace.notes, notesDirty]);
  useEffect(() => { onDirty(notesDirty || Object.values(drafts).some(d => d.dirty)); }, [notesDirty, drafts, onDirty]);
  const current = drafts[selectedId]?.value ?? (workspace.profiles.find(p => p.id === selectedId) ? draftFor(workspace.profiles.find(p => p.id === selectedId)!) : undefined);
  const dirty = drafts[selectedId]?.dirty ?? false;
  const setField = <K extends keyof ProfileDraft>(key: K, value: ProfileDraft[K]) => { if (!current) return; setDrafts(d => ({ ...d, [selectedId]: { value: { ...current, [key]: value }, dirty: true } })); setValidation(''); };
  const newProfile = () => { const value = draftFor({ id: crypto.randomUUID(), name: '새 제작 기준', stage: '', kind: 'sprite', pixelSize: 1, width: null, height: null, alphaMode: 'preserve', alphaThreshold: 128, palette: [], notes: '' }); setDrafts(d => ({ ...d, [value.id]: { value, dirty: true } })); setSelectedId(value.id); setValidation(''); };
  async function save() {
    if (!current) return;
    const pixelSize = Number(current.pixelSize), width = current.width.trim() ? Number(current.width) : null, height = current.height.trim() ? Number(current.height) : null, alphaThreshold = Number(current.alphaThreshold);
    if (!current.name.trim()) { setValidation('제작 기준 이름을 입력해 주세요.'); return; }
    if (!current.pixelSize.trim() || !Number.isInteger(pixelSize) || pixelSize < 1 || pixelSize > 64) { setValidation('논리 픽셀 크기는 1~64px의 정수로 입력해 주세요.'); return; }
    if ([width, height].some(v => v !== null && (!Number.isInteger(v) || v < 1 || v > 16384))) { setValidation('너비와 높이는 비워 두거나 1~16,384px의 정수로 입력해 주세요.'); return; }
    if (!current.alphaThreshold.trim() || !Number.isInteger(alphaThreshold) || alphaThreshold < 1 || alphaThreshold > 255) { setValidation('알파 임계값은 1~255의 정수로 입력해 주세요.'); return; }
    const palette = [...new Set(current.palette.split(/[\s,;]+/).filter(Boolean).map(color => color.toUpperCase()))];
    if (palette.some(v => !/^#[0-9a-fA-F]{6}$/.test(v))) { setValidation('팔레트는 #RRGGBB 형식으로 입력해 주세요. 줄바꿈이나 쉼표로 색을 구분할 수 있습니다.'); return; }
    if (palette.length > 256) { setValidation('팔레트는 서로 다른 색을 최대 256개까지 지정할 수 있습니다.'); return; }
    const profile: RuleProfile = { ...current, name: current.name.trim(), pixelSize, width, height, alphaThreshold, palette };
    if (await run('제작 기준 저장 및 재검사', () => api.saveProfile(profile))) setDrafts(d => ({ ...d, [profile.id]: { value: draftFor(profile), dirty: false } }));
  }
  const profileEntries = Object.values(drafts);
  const knownIds = new Set(profileEntries.map(d => d.value.id));
  const entries = [...profileEntries, ...workspace.profiles.filter(p => !knownIds.has(p.id)).map(p => ({ value: draftFor(p), dirty: false }))];
  return <div className="rules-layout"><aside className="rules-sidebar"><div className="section-heading"><h2>제작 기준</h2><button className="icon-button" title="새 제작 기준" aria-label="새 제작 기준" onClick={newProfile} disabled={busy}><Plus size={17} /></button></div><div className="profile-list">{entries.map(({ value, dirty: changed }) => <button key={value.id} className={selectedId === value.id ? 'active' : ''} onClick={() => { setSelectedId(value.id); setValidation(''); }}><FileText size={16} /><span><strong>{value.name}</strong><small>{value.stage || '단계 미지정'} · {value.kind}</small></span>{changed && <span className="dirty-dot" />}</button>)}</div><details className="rule-details"><summary>규격 적용 범위</summary><p className="helper">각 에셋에 지정한 기준만 적용됩니다. Stage03의 4×4 논리 픽셀과 1672×944 배경 규격은 다른 단계·종류에 공통 적용하지 않습니다.</p></details></aside>
    <main className="rule-editor scroll-panel">{current && <>
      <header className="page-heading"><h1>{current.name}</h1>{dirty && <span className="dirty-label">저장 전</span>}</header>
      <div className="profile-form">
        <Field label="기준 이름"><input value={current.name} onChange={e => setField('name', e.target.value)} disabled={busy} /></Field>
        <div className="field-row"><Field label="단계"><input value={current.stage} onChange={e => setField('stage', e.target.value)} disabled={busy} /></Field><Field label="에셋 종류"><select value={current.kind} onChange={e => setField('kind', e.target.value as RuleProfile['kind'])} disabled={busy}><option value="sprite">스프라이트</option><option value="background">배경</option><option value="ui">UI</option><option value="effect">이펙트</option></select></Field></div>
        <Field label="논리 픽셀 크기 (px)" hint="1: 실제 픽셀 단위"><input type="number" min="1" max="64" step="1" value={current.pixelSize} onChange={e => setField('pixelSize', e.target.value)} disabled={busy} /></Field>
        <div className="field-row"><Field label="너비 (px)" hint="빈칸: 크기 제한 없음"><input type="number" min="1" max="16384" step="1" value={current.width} onChange={e => setField('width', e.target.value)} disabled={busy} /></Field><Field label="높이 (px)" hint="빈칸: 크기 제한 없음"><input type="number" min="1" max="16384" step="1" value={current.height} onChange={e => setField('height', e.target.value)} disabled={busy} /></Field></div>
        <details className="rule-details" key={current.id}><summary>투명도·팔레트 <span className="muted">· {current.alphaMode === 'preserve' ? '원본 알파 유지' : '이진 알파'}</span></summary>
          <div className="field-row"><Field label="알파 기준"><select value={current.alphaMode} onChange={e => setField('alphaMode', e.target.value as RuleProfile['alphaMode'])} disabled={busy}><option value="preserve">원본 알파 유지</option><option value="binary">이진 알파</option></select></Field><Field label="알파 임계값 (1–255)"><input type="number" min="1" max="255" step="1" value={current.alphaThreshold} onChange={e => setField('alphaThreshold', e.target.value)} disabled={busy} /></Field></div>
          <Field label="제한 팔레트" hint="#RRGGBB · 줄바꿈 또는 쉼표로 구분 / 빈칸: 제한 없음"><textarea className="mono" rows={5} value={current.palette} onChange={e => setField('palette', e.target.value)} disabled={busy} spellCheck={false} /></Field>
          <div className="palette-swatches">{current.palette.split(/[\s,;]+/).filter(v => /^#[0-9a-fA-F]{6}$/.test(v)).map((color, i) => <span key={`${color}-${i}`} title={color} style={{ backgroundColor: color }} />)}</div>
        </details>
        <Field label="제작 기준 메모" hint="형태·명암·테두리 등 직접 검토할 내용"><textarea rows={3} value={current.notes} onChange={e => setField('notes', e.target.value)} disabled={busy} /></Field>
        {validation && <p className="inline-error" role="alert">{validation}</p>}
        <div className="form-actions"><button className="button primary" disabled={busy || !dirty} onClick={() => void save()}><Save size={15} />기준 저장 · 재검사</button><button className="button" disabled={busy} onClick={() => { const saved = workspace.profiles.find(p => p.id === current.id); setPrompt(buildPrompt(saved, workspace.notes)); }}><Copy size={15} />저장된 기준으로 프롬프트</button></div>
        {dirty && <p className="helper">저장하면 연결된 에셋을 다시 검사합니다.</p>}
      </div>
    </>}</main>
    <aside className="project-notes scroll-panel"><Field label="프로젝트 공통 메모"><textarea rows={6} value={notes} onChange={e => { setNotes(e.target.value); setNotesDirty(true); }} disabled={busy} /></Field><button className="button" disabled={busy || !notesDirty} onClick={() => void run('공통 메모 저장', () => api.saveProjectNotes(notes)).then(ok => { if (ok) setNotesDirty(false); })}><Save size={15} />메모 저장{notesDirty && <span className="dirty-dot" />}</button><div className="form-divider" /><h2>참고용 이미지</h2>{workspace.assets.some(a => a.reference) ? <ul className="reference-list">{workspace.assets.filter(a => a.reference).map(a => <li key={a.id}><button onClick={() => onSelectAsset(a.id)}><img src={api.imageUrl(a.filePath, a.currentHash)} alt="" /><span>{a.name}<small>{a.width} × {a.height}</small></span></button></li>)}</ul> : <p className="helper">이미지 옆 ‘리소스’에서 참고용으로 지정할 수 있습니다.</p>}</aside>
    {prompt && <div className="modal-backdrop"><section className="modal prompt-modal" role="dialog" aria-modal="true" aria-labelledby="prompt-title"><h2 id="prompt-title">제작 기준 프롬프트</h2><p className="helper">복사해서 기존 ChatGPT 작업에도 사용할 수 있습니다. 이 동작은 이미지를 전송하지 않습니다.</p><textarea rows={16} value={prompt} onChange={e => setPrompt(e.target.value)} aria-label="복사할 프롬프트" /><div className="form-actions"><button className="button primary" disabled={busy} onClick={() => void runAction('프롬프트 복사', async () => { await api.copyText(prompt); })}><Copy size={15} />복사</button><button className="button" onClick={() => setPrompt('')}>닫기</button></div></section></div>}
  </div>;
}
