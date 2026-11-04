import { useState } from 'react';
import { Copy, RefreshCw, Save } from 'lucide-react';
import type { AppSettings, CodexModel } from '../domain';
import * as api from '../bridge';
import { Field } from './ui';
import type { RunAction } from './ui';
import { Choice, DetailPanel } from './Interaction';
import '../workbench-settings.css';

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
  const manualModel = !!draft?.model && !selected;
  const savedModel = models.find(model => model.id === saved?.model.trim());
  return <section className="model-settings quiet-model-settings" aria-label="Codex 모델 설정">
    {loading ? <p className="helper" role="status">저장된 설정을 불러오는 중입니다.</p> : draft ? <>
      <div className="model-selection-summary">
        <span className="field-label">현재 적용 중</span>
        <strong>{saved ? savedModel?.displayName || saved.model || 'Codex 기본 모델' : '저장된 설정 없음'}</strong>
        {saved && <span>{effortNames[saved.effort]} 추론{saved.model && savedModel?.displayName && <code>{saved.model}</code>}</span>}
      </div>
      <div className="model-settings-toolbar"><span>다음 검토에 사용할 설정</span><button className="text-button" disabled={busy || catalogLoading} onClick={onCatalog}><RefreshCw size={13} className={catalogLoading ? 'spin' : ''} />{catalogLoading ? '목록 불러오는 중' : '목록 새로고침'}</button></div>
      <Choice<string> label="모델" value={draft.model} disabled={busy} onChange={model => { setDirectEntry(false); onChange({ ...draft, model }); }} options={[
        { value: '', label: 'Codex 기본 모델' },
        ...models.map(model => ({ value: model.id, label: model.displayName || model.id, detail: model.displayName ? model.id : undefined })),
        ...(manualModel ? [{ value: draft.model, label: draft.model, detail: '직접 지정' }] : []),
      ]} />
      <Choice<AppSettings['effort']> label="추론 수준" value={draft.effort} disabled={busy} onChange={effort => onChange({ ...draft, effort })} options={[
        ...(!availableEfforts.includes(draft.effort) ? [{ value: draft.effort, label: effortNames[draft.effort], detail: '이 모델에서 지원하지 않음' }] : []),
        ...availableEfforts.map(effort => ({ value: effort, label: effortNames[effort], detail: effort })),
      ]} />
      {validation && <p className="inline-error" role="alert">{validation}</p>}
      <DetailPanel className="model-advanced" title="모델 ID 직접 지정" open={directEntry} onOpenChange={setDirectEntry}>
        <Field label="모델 ID" hint="목록에 없는 모델은 실행 시 사용 가능 여부를 확인합니다. 기본 모델은 위 목록에서 선택하세요."><input value={draft.model} placeholder="사용할 모델 ID" spellCheck={false} disabled={busy} onChange={event => onChange({ ...draft, model: event.target.value })} /></Field>
      </DetailPanel>
      {selected?.description && <DetailPanel className="model-details" title="모델 정보"><p className="helper model-description">{selected.description}</p></DetailPanel>}
      <div className="model-save-footer">
        <p className="helper">저장만으로 AI 검토를 실행하거나 이미지를 전송하지 않습니다.</p>
        <div className="form-actions"><span className="muted">{dirty ? '저장 전 변경사항 있음' : '저장된 설정과 같음'}</span><button className="button primary" disabled={busy || !dirty || !!validation || (directEntry && !draft.model.trim())} onClick={onSave}><Save size={14} />설정 저장</button></div>
      </div>
    </> : <button className="button" disabled={busy} onClick={onReload}>설정 다시 불러오기</button>}
    {catalogError && <p className="inline-error" role="alert">모델 목록을 불러오지 못했습니다: {catalogError}<br />저장된 설정은 유지됩니다. 목록을 새로고침하거나 모델 ID를 직접 지정하세요.</p>}
    {catalogLoaded && !catalogLoading && !catalogError && models.length === 0 && <p className="helper">계정에서 반환한 모델이 없습니다. 기본 모델 또는 직접 지정한 ID를 사용하세요.</p>}
    {catalogLoading && (!draft || loading) && <p className="helper" role="status">계정 모델 목록을 불러오는 중입니다.</p>}
    {error && <p className="inline-error" role="alert">{error}</p>}
    <DetailPanel className="cli-help" title="터미널에서 사용하기"><p className="helper">CLI가 설치된 터미널에서 실행하세요.</p>{['taid --help', 'taid settings get'].map(command => <div className="command-row" key={command}><code>{command}</code><button className="icon-button" title="명령 복사" aria-label={`${command} 복사`} disabled={busy} onClick={() => void run('CLI 명령 복사', () => api.copyText(command))}><Copy size={13} /></button></div>)}</DetailPanel>
  </section>;
}
