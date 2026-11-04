import { useEffect, useMemo, useRef, useState } from 'react';
import { Columns2, Crosshair, Image, Layers2, Maximize, Pause, Play, ZoomIn } from 'lucide-react';
import type { Asset, Workspace } from '../domain';
import * as api from '../bridge';
import { Empty } from './ui';

type Background = 'checker' | 'dark' | 'light' | 'reference';
type View = 'result' | 'original' | 'compare';
type SequenceStage = { minX: number; minY: number; width: number; height: number };
function ImagePlane({ asset, original, zoom, background, reference, pivot, onion, stage }: { asset: Asset; original: boolean; zoom: number | 'fit'; background: Background; reference?: Asset; pivot: boolean; onion?: Asset; stage?: SequenceStage }) {
  const container = useRef<HTMLDivElement>(null);
  const [box, setBox] = useState({ width: 600, height: 500 });
  const [size, setSize] = useState({ width: asset.width, height: asset.height });
  const [imageError, setImageError] = useState(false);
  const path = original ? asset.originalPath : asset.filePath;
  const hash = original ? asset.originalHash : asset.currentHash;
  useEffect(() => {
    if (!container.current) return;
    const observer = new ResizeObserver(([entry]) => setBox({ width: entry.contentRect.width, height: entry.contentRect.height }));
    observer.observe(container.current);
    return () => observer.disconnect();
  }, []);
  useEffect(() => { setImageError(false); }, [path, hash]);
  const bounds = !original && stage ? stage : { minX: -asset.pivotX, minY: -asset.pivotY, width: size.width, height: size.height };
  const scale = zoom === 'fit' ? Math.max(0.01, Math.min((box.width - 64) / bounds.width, (box.height - 64) / bounds.height)) : zoom;
  const anchorX = -bounds.minX * scale;
  const anchorY = -bounds.minY * scale;
  const style = background === 'reference' && reference ? { backgroundImage: `url("${api.imageUrl(reference.filePath, reference.currentHash)}")` } : undefined;
  return <div ref={container} className={`image-plane canvas-${background === 'checker' ? 'stage' : background}`} style={style}>
    <div className="image-position" style={{ minWidth: Math.max(box.width, bounds.width * scale + 64), minHeight: Math.max(box.height, bounds.height * scale + 64) }}>
      <div className="pixel-image" style={{ width: bounds.width * scale, height: bounds.height * scale }}>
        <img className={background === 'checker' ? 'image-checker' : undefined} alt={`${asset.name} ${original ? '원본' : '현재 이미지'}`} draggable={false} src={api.imageUrl(path, hash)} style={original ? undefined : { width: asset.width * scale, height: asset.height * scale, left: anchorX - asset.pivotX * scale, top: anchorY - asset.pivotY * scale }} onLoad={e => setSize({ width: e.currentTarget.naturalWidth, height: e.currentTarget.naturalHeight })} onError={() => setImageError(true)} />
        {onion && !original && <img className="onion-image" alt="이전 프레임 반투명 겹침" src={api.imageUrl(onion.filePath, onion.currentHash)} style={{ width: onion.width * scale, height: onion.height * scale, left: anchorX - onion.pivotX * scale, top: anchorY - onion.pivotY * scale }} />}
        {pivot && !original && <span className="pivot-guide" aria-label={`공통 피벗 · 이미지 좌표 ${asset.pivotX}, ${asset.pivotY}`} style={{ left: anchorX, top: anchorY }}><span /></span>}
      </div>
      {imageError && <p className="canvas-error" role="alert">이미지를 읽지 못했습니다. 원본 경로와 파일을 확인해 주세요.</p>}
    </div>
    <span className="canvas-caption">{original ? '원본 · 중앙 정렬' : '현재 이미지 · 공통 피벗 정렬'} · {original ? size.width : asset.width} × {original ? size.height : asset.height} · {zoom === 'fit' ? `${Math.round(scale * 100)}%` : `${zoom}×`}</span>
  </div>;
}
export function Preview({ workspace, asset, onActive }: { workspace: Workspace; asset?: Asset; onActive: (id: string) => void }) {
  const [view, setView] = useState<View>('result');
  const [zoom, setZoom] = useState<number | 'fit'>('fit');
  const [background, setBackground] = useState<Background>('checker');
  const [referenceId, setReferenceId] = useState('');
  const [pivot, setPivot] = useState(false);
  const [onion, setOnion] = useState(false);
  const [playing, setPlaying] = useState(false);
  const [playIndex, setPlayIndex] = useState(0);
  const frames = useMemo(() => asset?.sequence ? workspace.assets.filter(a => !a.reference && a.sequence === asset.sequence).sort((a, b) => a.frameIndex - b.frameIndex || a.layer - b.layer || a.name.localeCompare(b.name)) : [], [workspace.assets, asset?.sequence]);
  const stage = useMemo<SequenceStage | undefined>(() => {
    const members = frames.length ? frames : asset ? [asset] : [];
    if (!members.length) return undefined;
    let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
    for (const frame of members) {
      minX = Math.min(minX, -frame.pivotX);
      minY = Math.min(minY, -frame.pivotY);
      maxX = Math.max(maxX, frame.width - frame.pivotX);
      maxY = Math.max(maxY, frame.height - frame.pivotY);
    }
    return { minX, minY, width: Math.max(1, maxX - minX), height: Math.max(1, maxY - minY) };
  }, [frames, asset]);
  const frameKey = frames.map(f => `${f.id}:${f.durationMs}`).join('|');
  useEffect(() => { setPlaying(false); setPlayIndex(0); }, [asset?.id]);
  useEffect(() => { if (view === 'original') setPlaying(false); }, [view]);
  useEffect(() => {
    if (!playing || frames.length < 2) return;
    const timer = window.setTimeout(() => setPlayIndex(i => (i + 1) % frames.length), frames[playIndex % frames.length].durationMs);
    return () => window.clearTimeout(timer);
  }, [playing, playIndex, frameKey, frames.length]);
  const shown = playing && frames.length ? frames[playIndex % frames.length] : asset;
  const currentIndex = frames.findIndex(f => f.id === shown?.id);
  const previous = currentIndex > 0 ? frames[currentIndex - 1] : frames.length > 1 ? frames[frames.length - 1] : undefined;
  const references = workspace.assets.filter(a => a.reference);
  const reference = references.find(a => a.id === referenceId) ?? references[0];
  if (!asset || !shown) return <section className="preview-region"><Empty title="이미지를 선택하세요" icon={<Image size={30} />}><p>왼쪽 목록에서 검토할 이미지를 선택하세요.</p></Empty></section>;
  return <section className="preview-region" aria-label="에셋 미리보기">
    <div className="preview-toolbar">
      <div className="segmented" aria-label="비교 보기">{([['result', '현재'], ['original', '원본'], ['compare', '나란히']] as const).map(([id, name]) => <button key={id} className={view === id ? 'active' : ''} onClick={() => setView(id)} aria-pressed={view === id}>{id === 'compare' && <Columns2 size={14} />}{name}</button>)}</div>
      <div className="preview-tools"><label className="compact-label"><ZoomIn size={15} /><select aria-label="배율" value={zoom} onChange={e => setZoom(e.target.value === 'fit' ? 'fit' : Number(e.target.value))}><option value="fit">화면 맞춤</option>{[1, 2, 3, 4, 6, 8, 12, 16].map(z => <option key={z} value={z}>{z}×</option>)}</select></label><button className={`icon-button ${pivot ? 'active' : ''}`} aria-label="피벗 가이드" title="피벗 가이드" aria-pressed={pivot} onClick={() => setPivot(v => !v)}><Crosshair size={16} /></button><button className="icon-button" aria-label="화면 맞춤" title="화면 맞춤" onClick={() => setZoom('fit')}><Maximize size={16} /></button></div>
    </div>
    <div className={`preview-canvases ${view === 'compare' ? 'compare' : ''}`}>
      {view === 'compare' ? <><ImagePlane asset={asset} original zoom={zoom} background={background} reference={reference} pivot={false} /><ImagePlane asset={shown} original={false} zoom={zoom} background={background} reference={reference} pivot={pivot} onion={onion ? previous : undefined} stage={stage} /></> : <ImagePlane asset={view === 'original' ? asset : shown} original={view === 'original'} zoom={zoom} background={background} reference={reference} pivot={pivot} onion={onion ? previous : undefined} stage={stage} />}
    </div>
    <div className="canvas-settings"><span className="muted">바탕</span><div className="background-options">{([['checker', '투명 체크'], ['dark', '어둡게'], ['light', '밝게']] as const).map(([id, label]) => <button key={id} className={`background-swatch canvas-${id} ${background === id ? 'active' : ''}`} title={label} aria-label={label} aria-pressed={background === id} onClick={() => setBackground(id)} />)}</div>{references.length > 0 && <select aria-label="참고 이미지 바탕" value={background === 'reference' ? reference?.id : ''} onChange={e => { setReferenceId(e.target.value); setBackground(e.target.value ? 'reference' : 'checker'); }}><option value="">참고 이미지 바탕</option>{references.map(a => <option key={a.id} value={a.id}>{a.name}</option>)}</select>}<span className="canvas-filename" title={shown.name}>{shown.name}.png</span></div>
    {frames.length > 1 && <div className="animation-strip">
      <div className="animation-heading">
        <button className={`button small ${playing ? 'active' : ''}`} onClick={() => { if (view === 'original') setView('result'); setPlaying(v => !v); }} aria-label={playing ? '애니메이션 정지' : '애니메이션 정상 속도 재생'}>{playing ? <Pause size={14} /> : <Play size={14} />}{playing ? '정지' : '재생'}</button>
        <span className="sequence-label" title={asset.sequence}>{asset.sequence}</span><span className="muted">{frames.length}프레임 · 1× 속도</span>
        <button className={`button small onion-toggle ${onion ? 'active' : ''}`} onClick={() => setOnion(v => !v)} aria-pressed={onion}><Layers2 size={14} />어니언 스킨</button>
      </div>
      <div className="frame-list" aria-label="애니메이션 프레임">{frames.map(frame => <button key={frame.id} className={`frame-button ${shown.id === frame.id ? 'active' : ''}`} onClick={() => { setPlaying(false); onActive(frame.id); }} title={`${frame.name} · 레이어 ${frame.layer}`}><span className="frame-image canvas-checker"><img src={api.imageUrl(frame.filePath, frame.currentHash)} alt={frame.name} /></span><span>#{frame.frameIndex}</span><small>{frame.durationMs}ms</small></button>)}</div>
    </div>}
  </section>;
}
