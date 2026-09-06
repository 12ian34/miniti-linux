interface LevelMeterProps {
  label: string;
  level: number;
  active: boolean;
  tone?: "mic" | "system";
}

const BARS = 24;

/** Dual labelled waveform: mic green, system blue (macOS `SourceWaveform`). */
export function LevelMeter({ label, level, active, tone = "mic" }: LevelMeterProps) {
  // pow(level, 0.2) so quiet mic signals (~0.003 RMS) still show movement.
  const shaped = Math.min(1, Math.pow(Math.max(0, level), 0.2));
  const lit = Math.round(shaped * BARS);
  return (
    <div className={`meter ${tone}`}>
      <div className="meter-head">
        <span className={`dot ${active ? "dot-ok" : "dot-off"}`} />
        <span className="k">{label}</span>
      </div>
      <div className="wave">
        {Array.from({ length: BARS }, (_, i) => (
          <span
            key={i}
            className={`wave-bar ${i < lit ? "on" : ""}`}
            style={{ height: `${30 + (i < lit ? Math.min(70, shaped * 70 * (0.6 + ((i * 7) % 5) / 10)) : 0)}%` }}
          />
        ))}
      </div>
    </div>
  );
}
