import { useCallback, useEffect, useRef } from 'react';
import * as api from './bridge';

type ImagePasteOptions = {
  onPaths: (paths: string[]) => Promise<void>;
  onError: (message: string) => void;
  disabled: boolean;
};

function editableTarget(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) return false;
  return !!target.closest('input, textarea, [role~="textbox"]')
    || (target instanceof HTMLElement && target.isContentEditable);
}

/** The resolved path lease belongs to onPaths until import succeeds or intake is cancelled. */
export function useImagePaste(options: ImagePasteOptions): () => Promise<void> {
  const latest = useRef(options);
  latest.current = options;
  const inFlight = useRef(false);
  const mounted = useRef(false);
  const composing = useRef(false);

  const pasteImage = useCallback(async () => {
    if (latest.current.disabled || inFlight.current) return;
    inFlight.current = true;
    let path: string | undefined;
    let handedOff = false;
    try {
      path = await api.captureClipboardImage();
      if (!mounted.current) {
        const orphanedPath = path;
        path = undefined;
        await api.releaseClipboardImages([orphanedPath]);
        return;
      }
      await latest.current.onPaths([path]);
      handedOff = true;
    } catch (error) {
      let message = error instanceof Error ? error.message : String(error);
      if (path && !handedOff) {
        try { await api.releaseClipboardImages([path]); }
        catch (cleanupError) {
          const detail = cleanupError instanceof Error ? cleanupError.message : String(cleanupError);
          message += ` · 임시 이미지 정리 실패: ${detail}`;
        }
      }
      if (mounted.current) latest.current.onError(message);
    } finally {
      inFlight.current = false;
    }
  }, []);

  useEffect(() => {
    mounted.current = true;
    const compositionStart = () => { composing.current = true; };
    const compositionEnd = () => { composing.current = false; };
    const blur = () => { composing.current = false; };
    const keydown = (event: KeyboardEvent) => {
      if (event.defaultPrevented || event.repeat || event.isComposing || composing.current || latest.current.disabled) return;
      if (!(event.ctrlKey || event.metaKey) || event.altKey || event.shiftKey || event.key.toLowerCase() !== 'v') return;
      if (event.composedPath().some(editableTarget) || editableTarget(document.activeElement)) return;
      const dialogOpen = Array.from(document.querySelectorAll<HTMLElement>('[role~="dialog"]')).some(dialog => {
        if (dialog.closest('[hidden], [inert]') || dialog.getClientRects().length === 0) return false;
        const style = window.getComputedStyle(dialog);
        return style.display !== 'none' && style.visibility !== 'hidden' && style.visibility !== 'collapse';
      });
      if (dialogOpen) return;
      event.preventDefault();
      void pasteImage();
    };
    window.addEventListener('keydown', keydown);
    window.addEventListener('compositionstart', compositionStart);
    window.addEventListener('compositionend', compositionEnd);
    window.addEventListener('blur', blur);
    return () => {
      mounted.current = false;
      composing.current = false;
      window.removeEventListener('keydown', keydown);
      window.removeEventListener('compositionstart', compositionStart);
      window.removeEventListener('compositionend', compositionEnd);
      window.removeEventListener('blur', blur);
    };
  }, [pasteImage]);

  return pasteImage;
}
