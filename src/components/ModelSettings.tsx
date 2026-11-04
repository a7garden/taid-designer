import { useState } from 'react';
import { Copy, RefreshCw, Save } from 'lucide-react';
import type { AppSettings, CodexModel } from '../domain';
import * as api from '../bridge';
import { Field } from './ui';
import type { RunAction } from './ui';

const efforts: AppSettings['effort'][] = ['none', 'minimal', 'low', 'medium', 'high', 'xhigh'];
const effortNames: Record<AppSettings['effort'], string> = { none: '없음', minimal: '최소', low: '낮음', medium: '보통', high: '높음', xhigh: '매우 높음' };
export function modelValidation(settings: AppSettings | null, models: CodexModel[]): string {
  const model = models.find(item => item.id === settings?.model.trim());
  return settings && model && model.efforts.length > 0 && !model.efforts.includes(settings.effort)
    ? `${model.displayName || model.id}에서 추론 수준 ‘${effortNames[settings.effort]}’을 지원하지 않습니다. 지원하는 수준을 선택해 저장하세요.` : '';
}
export interface ModelSettingsProps {
  saved: AppSettings | null; draft: AppSettings | null; models: CodexModel[];
  loading: boolean; catalogLoading: boolean; catalogLoaded: boolean; error: string; catalogError: string; busy: boolean;
  onChange: (settings: AppSettings) => void; onSave: () => void; onReload: () => void; onCatalog: () => void; run: RunAction;
}
export function ModelSettings({ saved, draft, models, loading, catalogLoading, catalogLoaded, error, catalogError, busy, onChange, onSave, onReload, onCatalog, run }: ModelSettingsProps) {
  const [directEntry, setDirectEntry] = useState(false);
  const selected = models.find(model => model.id === draft?.model.trim());
  const availableEfforts = selected?.efforts.length ? efforts.filter(effort => selected.efforts.includes(effort)) : efforts;
  const validation = modelValidation(draft, models);
  const dirty = !!draft && (!saved || saved.model !== draft.model || saved.effort !== draft.effort);
  return <section className="model-settings" aria-label="Codex 모델 설정">
    <div className="model-settings-toolbar"><span>계정 모델 목록</span><button className="text-button" disabled={busy || catalogLoading} onClick={onCatalog}><RefreshCw size={12} className={catalogLoading ? 'spin' : ''} />새로고침</button></div>
    {loading ? <p className="helper">저장된 설정을 불러오는 중입니다.</p> : draft ? <>
      <Field label="모델"><select value={directEntry || (draft.model && !selected) ? '__manual__' : draft.model} disabled={busy} onChange={event => { const value = event.target.value; setDirectEntry(value === '__manual__'); if (value !== '__manual__') onChange({ ...draft, model: value }); }}>
        <option value="">Codex 기본 모델</option>
        {models.map(model => <option key={model.id} value={model.id}>{model.displayName || model.id}</option>)}
        <option value="__manual__">모델 ID 직접 입력</option>
      </select></Field>
      {(directEntry || (draft.model && !selected)) && <Field label="모델 ID" hint="계정 목록에 없는 모델도 ID로 지정할 수 있습니다. 사용 가능 여부는 Codex 실행 시 확인됩니다."><input value={draft.model} placeholder="모델 ID" spellCheck={false} disabled={busy} onChange={event => onChange({ ...draft, model: event.target.value })} /></Field>}
      {selected?.description && <details className="model-details"><summary>선택한 모델 정보</summary><p className="helper model-description">{selected.description}</p></details>}
      <Field label="추론 수준"><select value={draft.effort} disabled={busy} onChange={event => onChange({ ...draft, effort: event.target.value as AppSettings['effort'] })}>
        {!availableEfforts.includes(draft.effort) && <option value={draft.effort}>{effortNames[draft.effort]} · 현재 저장값, 지원 안 됨</option>}
        {availableEfforts.map(effort => <option key={effort} value={effort}>{effortNames[effort]} ({effort})</option>)}
      </select></Field>
      {validation && <p className="inline-error" role="alert">{validation}</p>}
      <div className="form-actions"><button className="button primary" disabled={busy || !dirty || !!validation || (directEntry && !draft.model.trim())} onClick={onSave}><Save size={14} />설정 저장</button>{dirty && <span className="muted">저장 전 편집값 있음</span>}</div>
      {saved && <p className="settings-current">저장값: <code>{saved.model || 'Codex 기본 모델'}</code> · {effortNames[saved.effort]} ({saved.effort})</p>}
    </> : <button className="button small" disabled={busy} onClick={onReload}>설정 다시 불러오기</button>}
    {error && <p className="inline-error" role="alert">{error}</p>}
    {catalogError && <p className="inline-error">모델 목록을 불러오지 못했습니다: {catalogError}<br />저장된 모델은 유지됩니다. 직접 ID를 입력하거나 목록을 다시 불러오세요.</p>}
    {catalogLoading && <p className="helper">계정 모델 목록을 불러오는 중입니다.</p>}
    {catalogLoaded && !catalogLoading && !catalogError && models.length === 0 && <p className="helper">계정에서 반환한 모델이 없습니다. 기본 모델을 쓰거나 ID를 직접 입력하세요.</p>}
    <details className="cli-help"><summary>CLI에서 설정·사용</summary><p className="helper">CLI가 설치된 터미널에서 실행하세요.</p>{['taid --help', 'taid settings get'].map(command => <div className="command-row" key={command}><code>{command}</code><button className="icon-button" title="명령 복사" aria-label={`${command} 복사`} disabled={busy} onClick={() => void run('CLI 명령 복사', () => api.copyText(command))}><Copy size={13} /></button></div>)}</details>
  </section>;
}
