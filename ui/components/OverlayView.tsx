import { useState, useEffect, useRef } from "react";

export function OverlayView() {
  const [processing, setProcessing] = useState(false);
  const barsRef = useRef<(HTMLSpanElement | null)[]>([]);
  const targetLevelRef = useRef(0);
  const smoothLevelRef = useRef(0);
  const rafRef = useRef<number>(0);

  useEffect(() => {
    const animate = () => {
      const target = targetLevelRef.current;
      const current = smoothLevelRef.current;
      if (target > current) {
        smoothLevelRef.current += (target - current) * 0.4;
      } else {
        smoothLevelRef.current *= 0.88;
      }
      const level = smoothLevelRef.current;

      const t = Date.now();
      barsRef.current.forEach((bar, i) => {
        if (!bar) return;
        const idle = Math.sin(t / 300 + i * 1.5) * 0.3 + 0.5;
        const voice = Math.sin(t / 100 + i * 2.0) * 0.3 + 0.7;
        const h = 3 + idle * 4 + level * 18 * voice;
        bar.style.height = `${h}px`;
        bar.style.opacity = `${0.4 + level * 0.6}`;
      });
      rafRef.current = requestAnimationFrame(animate);
    };
    rafRef.current = requestAnimationFrame(animate);

    (window as any).__overlaySetProcessing = (v: boolean) => setProcessing(v);
    (window as any).__overlaySetLevel = (v: number) => { targetLevelRef.current = v; };

    return () => {
      cancelAnimationFrame(rafRef.current);
      delete (window as any).__overlaySetProcessing;
      delete (window as any).__overlaySetLevel;
    };
  }, []);

  return (
    <div className="overlay-container">
      <div className={`overlay-pill${processing ? " processing" : ""}`}>
        {processing ? (
          <div className="processing-dots">
            <span className="processing-dot" />
            <span className="processing-dot" />
            <span className="processing-dot" />
          </div>
        ) : (
          <div className="waveform">
            {Array.from({ length: 5 }).map((_, i) => (
              <span
                key={i}
                className="waveform-bar"
                ref={(el) => { barsRef.current[i] = el; }}
              />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
