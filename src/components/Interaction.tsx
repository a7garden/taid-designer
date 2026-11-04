import { useId, useState } from 'react';
import type { ReactNode } from 'react';
import { createPortal } from 'react-dom';
import { ArrowUpRight, Search, X } from 'lucide-react';
import { useModalKeyboard } from './ui';
import '../Interaction.css';

export function Choice<T extends string | number>({ label, value, onChange, options, disabled = false, compact = false }: {
  label: string; value: T; onChange: (value: T) => void;
  options: readonly { value: T; label: string; detail?: string }[]; disabled?: boolean; compact?: boolean;
}) {
  const [query, setQuery] = useState('');
  const labelId = useId();
  const searchable = options.length > 8;
  const shown = searchable ? options.filter(option => `${option.label} ${option.detail ?? ''}`.toLocaleLowerCase().includes(query.toLocaleLowerCase())) : options;
  return <div className={`direct-choice ${compact ? 'compact' : ''}`}>
    <span className="direct-choice-label" id={labelId}>{label}</span>
    {searchable && <div className="choice-search"><Search size={14} /><input aria-label={`${label} 검색`} value={query} onChange={event => setQuery(event.target.value)} placeholder="이름으로 검색" disabled={disabled} /></div>}
    <div className={`choice-options ${searchable ? 'searchable' : ''}`} role="group" aria-labelledby={labelId}>
      {shown.map(option => <button type="button" key={option.value} className={`choice-option ${option.value === value ? 'selected' : ''}`} aria-pressed={option.value === value} disabled={disabled} onClick={() => onChange(option.value)}><span>{option.label}</span>{option.detail && <small>{option.detail}</small>}</button>)}
      {!shown.length && <p className="choice-empty">검색 결과가 없습니다.</p>}
    </div>
  </div>;
}

export function DetailPanel({ title, children, className = '', open, onOpenChange, trigger, triggerLabel, disabled = false }: {
  title: string; children: ReactNode; className?: string; open?: boolean; onOpenChange?: (open: boolean) => void; trigger?: ReactNode; triggerLabel?: string; disabled?: boolean;
}) {
  const [localOpen, setLocalOpen] = useState(false);
  const visible = open ?? localOpen;
  const titleId = useId();
  const change = (next: boolean) => { setLocalOpen(next); onOpenChange?.(next); };
  useModalKeyboard(visible, () => change(false));
  return <div className={`detail-entry ${className}`}>
    <button type="button" className="detail-link" aria-label={triggerLabel} aria-haspopup="dialog" aria-expanded={visible} disabled={disabled} onClick={() => change(true)}>{trigger ?? <><span>{title}</span><ArrowUpRight size={14} /></>}</button>
    {createPortal(<div className="detail-backdrop" hidden={!visible}>
      <section className="detail-sheet" role={visible ? 'dialog' : undefined} aria-modal={visible || undefined} aria-labelledby={titleId}>
        <header><h2 id={titleId}>{title}</h2><button type="button" className="icon-button" aria-label={`${title} 닫기`} onClick={() => change(false)}><X size={18} /></button></header>
        <div className={`detail-content ${className}`}>{children}</div>
        <footer><button type="button" className="button" onClick={() => change(false)}>작업으로 돌아가기</button></footer>
      </section>
    </div>, document.body)}
  </div>;
}

export function ToggleAction({ label, description, active, onChange, disabled = false }: {
  label: string; description?: string; active: boolean; onChange: (active: boolean) => void; disabled?: boolean;
}) {
  return <button type="button" className={`toggle-action ${active ? 'active' : ''}`} aria-pressed={active} disabled={disabled} onClick={() => onChange(!active)}><span><strong>{label}</strong>{description && <small>{description}</small>}</span><span className="toggle-action-state">{active ? '포함' : '제외'}</span></button>;
}
