import { useEffect, useRef, useState } from "react";
import * as pdfjs from "pdfjs-dist";
import workerUrl from "pdfjs-dist/build/pdf.worker.min.mjs?url";

pdfjs.GlobalWorkerOptions.workerSrc = workerUrl;

interface Props {
  source: string;
  page: number;
  onPageChange: (page: number) => void;
  onPageCount: (count: number) => void;
  zoom: number;
}

export function PdfViewer({
  source,
  page,
  onPageChange,
  onPageCount,
  zoom
}: Props) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [document, setDocument] = useState<pdfjs.PDFDocumentProxy | null>(null);
  const [error, setError] = useState("");

  useEffect(() => {
    let cancelled = false;
    setError("");
    setDocument(null);

    const task = pdfjs.getDocument(source);
    void task.promise
      .then((loaded) => {
        if (cancelled) {
          void loaded.destroy();
          return;
        }
        setDocument(loaded);
        onPageCount(loaded.numPages);
        if (page > loaded.numPages) onPageChange(loaded.numPages);
      })
      .catch((reason: unknown) => setError(String(reason)));

    return () => {
      cancelled = true;
      void task.destroy();
    };
  }, [source]);

  useEffect(() => {
    if (!document || !canvasRef.current) return;
    let cancelled = false;
    let renderTask: pdfjs.RenderTask | null = null;

    void document.getPage(Math.max(1, page)).then((pdfPage) => {
      if (cancelled || !canvasRef.current) return;
      const viewport = pdfPage.getViewport({ scale: zoom });
      const canvas = canvasRef.current;
      const context = canvas.getContext("2d");
      if (!context) return;
      const ratio = window.devicePixelRatio || 1;
      canvas.width = Math.floor(viewport.width * ratio);
      canvas.height = Math.floor(viewport.height * ratio);
      canvas.style.width = `${Math.floor(viewport.width)}px`;
      canvas.style.height = `${Math.floor(viewport.height)}px`;
      const transform: [number, number, number, number, number, number] | undefined =
        ratio === 1 ? undefined : [ratio, 0, 0, ratio, 0, 0];
      renderTask = pdfPage.render({
        canvasContext: context,
        viewport,
        transform
      });
      return renderTask.promise;
    }).catch((reason: unknown) => {
      if (!cancelled) setError(String(reason));
    });

    return () => {
      cancelled = true;
      renderTask?.cancel();
    };
  }, [document, page, zoom]);

  if (error) {
    return <div className="viewer-message error">טעינת ה־PDF נכשלה: {error}</div>;
  }

  if (!document) {
    return <div className="viewer-message">טוען מסמך…</div>;
  }

  return (
    <div className="pdf-canvas-wrap">
      <canvas ref={canvasRef} aria-label={`עמוד ${page}`} />
    </div>
  );
}
