import { useState } from 'react';
import { FolderOutput, PackageCheck } from 'lucide-react';
import type { ExportResult, Workspace } from '../domain';
import * as api from '../bridge';
import { AssetStatus, Empty, isApproved } from './ui';
import type { RunAction } from './ui';
import '../workbench-settings.css';

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
  return <main className="handoff-page scroll-panel quiet-handoff">
    <header className="page-heading"><div><h1>승인본 내보내기</h1><p>디자인·QC 승인을 마친 PNG를 전달합니다.</p></div></header>
    <div className="handoff-toolbar">
      <div><strong>선택한 이미지</strong><span className="muted">{selected.length}개</span></div>
      <div className="form-actions"><button className="text-button" disabled={busy || !approved.length} onClick={() => setSelectedIds(approved.map(a => a.id))}>승인본 모두 선택 ({approved.length})</button><button className="text-button" disabled={busy || !selected.length} onClick={() => setSelectedIds([])}>선택 해제</button></div>
    </div>
    {selected.length ? <div className="table-scroll"><table className="handoff-table">
      <thead><tr><th><span className="sr-only">선택</span></th><th>파일</th><th>크기 / 기준</th><th>승인 상태</th></tr></thead>
      <tbody>{selected.map(a => <tr key={a.id}>
        <td className="handoff-selection"><input type="checkbox" checked aria-label={`${a.name} 내보내기 선택`} disabled={busy} onChange={() => setSelectedIds(selectedIds.filter(id => id !== a.id))} /></td>
        <td><div className="table-file"><img src={api.imageUrl(a.filePath, a.currentHash)} alt="" /><span><strong>{a.name}.png</strong><small>{a.processed ? '정리한 이미지' : '원본 유지'}</small></span></div>
          <details className="handoff-metadata"><summary>프레임·피벗 정보</summary><dl><div><dt>시퀀스</dt><dd>{a.sequence || '없음'}</dd></div><div><dt>프레임</dt><dd>#{a.frameIndex} · {a.durationMs}ms</dd></div><div><dt>피벗</dt><dd>{a.pivotX}, {a.pivotY}</dd></div><div><dt>레이어</dt><dd>{a.layer}</dd></div></dl></details>
        </td>
        <td>{a.width} × {a.height}<small>{workspace.profiles.find(p => p.id === a.profileId)?.name || '기준 없음'}</small></td>
        <td><AssetStatus asset={a} /></td>
      </tr>)}</tbody>
    </table></div> : <Empty title="내보낼 이미지를 선택하세요" icon={<PackageCheck size={28} />}><p>승인본을 모두 선택하거나 이미지 화면에서 개별 선택하세요.</p></Empty>}
    <details className="export-contents"><summary>파일 구성과 적용 범위</summary>
      <dl><div><dt>PNG</dt><dd>승인한 개별 이미지 · 아틀라스 아님</dd></div><div><dt>manifest.json</dt><dd>크기, 피벗, 프레임 시간, 레이어, 정제 기록</dd></div><div><dt>CSV</dt><dd>이미지별 메타데이터 목록</dd></div><div><dt>전달 안내</dt><dd>프로젝트 메모와 적용 시 주의사항</dd></div></dl>
      <p className="helper">Unity 프로젝트는 수정하지 않습니다.</p>
    </details>
    <footer className="handoff-export-bar">
      {rejected.length > 0 && <p className="inline-error" role="alert">{rejected.length}개는 내보낼 수 없습니다. 참고용 이미지를 제외하고 디자인·QC 승인과 파일 검사를 완료하세요.</p>}
      <div className="handoff-export-action"><div><strong>PNG {selected.length}개 + 메타데이터</strong><p>manifest.json · CSV · 전달 안내<br />다음 단계에서 저장 폴더를 선택합니다.</p></div><button className="button primary" disabled={busy || !selected.length || rejected.length > 0} onClick={() => void exportSelected()}><FolderOutput size={16} />{selected.length}개 내보내기</button></div>
    </footer>
    {result && <section className="export-result" aria-live="polite"><PackageCheck size={19} /><div><h2>{result.count}개 내보내기 완료</h2><p className="mono">{result.path}</p></div></section>}
  </main>;
}
