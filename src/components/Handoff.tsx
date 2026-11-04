import { useState } from 'react';
import { CheckCheck, FolderOutput, PackageCheck, X } from 'lucide-react';
import type { ExportResult, Workspace } from '../domain';
import * as api from '../bridge';
import { AssetStatus, Empty, isApproved } from './ui';
import type { RunAction } from './ui';

export function Handoff({ workspace, selectedIds, setSelectedIds, busy, run }: { workspace: Workspace; selectedIds: string[]; setSelectedIds: (ids: string[]) => void; busy: boolean; run: RunAction }) {
  const [result, setResult] = useState<ExportResult | null>(null);
  const selected = workspace.assets.filter(a => selectedIds.includes(a.id));
  const rejected = selected.filter(a => !isApproved(a));
  const approved = workspace.assets.filter(isApproved);
  async function exportSelected() {
    await run('승인본 전달 내보내기', async () => {
      const path = await api.chooseExport();
      if (!path) return;
      const exported = await api.exportAssets(path, selectedIds);
      setResult(exported);
    });
  }
  return <main className="handoff-page scroll-panel">
    <header className="page-heading">
      <div><h1>승인본 전달</h1><p>디자인과 QC를 모두 승인한 PNG만 내보냅니다.</p></div>
      <button className="button primary" disabled={busy || !selected.length || rejected.length > 0} onClick={() => void exportSelected()}><FolderOutput size={16} />{selected.length}개 내보내기</button>
    </header>
    <div className="handoff-toolbar">
      <div><strong>전달 목록</strong><span className="muted">{selected.length}개 선택</span></div>
      <div className="form-actions"><button className="button small" disabled={busy || !approved.length} onClick={() => setSelectedIds(approved.map(a => a.id))}><CheckCheck size={14} />승인 완료 {approved.length}개만 선택</button><button className="button small" disabled={busy || !selected.length} onClick={() => setSelectedIds([])}>선택 해제</button></div>
    </div>
    {rejected.length > 0 && <div className="warning-banner" role="alert"><strong>아직 전달할 수 없는 이미지 {rejected.length}개</strong><p>참고용 이미지는 목록에서 빼고, 승인 전 이미지는 검토 패널에서 디자인·QC 승인을 완료하세요. 파일 검사 오류가 있으면 내보낼 수 없습니다.</p></div>}
    {selected.length ? <div className="table-scroll"><table className="handoff-table">
      <thead><tr><th>파일</th><th>크기 / 기준</th><th>시퀀스 / 프레임</th><th>피벗 / 레이어</th><th>승인 상태</th><th><span className="sr-only">목록에서 빼기</span></th></tr></thead>
      <tbody>{selected.map(a => <tr key={a.id}>
        <td><div className="table-file"><img src={api.imageUrl(a.filePath, a.currentHash)} alt="" /><span><strong>{a.name}.png</strong><small>{a.processed ? '정리한 이미지' : '원본 유지'}</small></span></div></td>
        <td>{a.width} × {a.height}<small>{workspace.profiles.find(p => p.id === a.profileId)?.name}</small></td>
        <td>{a.sequence || '시퀀스 없음'}<small>#{a.frameIndex} · {a.durationMs}ms</small></td>
        <td>{a.pivotX}, {a.pivotY}<small>레이어 {a.layer}</small></td>
        <td><AssetStatus asset={a} /></td>
        <td><button className="icon-button" aria-label={`${a.name} 전달 목록에서 빼기`} disabled={busy} onClick={() => setSelectedIds(selectedIds.filter(id => id !== a.id))}><X size={15} /></button></td>
      </tr>)}</tbody>
    </table></div> : <Empty title="전달할 이미지를 선택하세요" icon={<PackageCheck size={30} />}><p>‘이미지로 돌아가기’에서 고르거나, 위에서 승인 완료 이미지만 선택하세요.</p></Empty>}
    <details className="export-contents"><summary>내보내는 파일과 적용 범위</summary>
      <dl><div><dt>PNG</dt><dd>승인한 개별 이미지 · 아틀라스 아님</dd></div><div><dt>manifest.json</dt><dd>크기, 피벗, 프레임 시간, 레이어, 정제 기록</dd></div><div><dt>CSV</dt><dd>이미지별 메타데이터 목록</dd></div><div><dt>전달 안내</dt><dd>프로젝트 메모와 적용 시 주의사항</dd></div></dl>
      <p className="helper">Unity 프로젝트는 수정하지 않습니다. 내보내기 위치는 매번 직접 선택합니다.</p>
    </details>
    {result && <section className="export-result" aria-live="polite"><PackageCheck size={22} /><div><h2>{result.count}개 이미지 내보내기 완료</h2><p className="mono">{result.path}</p></div></section>}
  </main>;
}
