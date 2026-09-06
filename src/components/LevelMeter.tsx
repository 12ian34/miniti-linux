interface LevelMeterProps {
  label: string;
  level: number;
  active: boolean;
  tone?: "mic" | "system";
}

const BANDS = 5;

/** Port of the macOS `SourceWaveform`: 5 bands, 3 px bars, 2 px gap, 18 px tall. */
export function LevelMeter({ label, level, active, tone = "mic" }: LevelMeterProps) {
  // pow(level, 0.2) so quiet mic signals (~0.003 RMS) still show movement.
  const shaped = active ? Math.min(1, Math.pow(Math.max(0, level), 0.2)) : 0;
  const weights = [0.55, 0.85, 1, 0.75, 0.5];
  return (
    <div className={`wave ${tone}`} title={label}>
      <span className="wave-label">{label}</span>
      <span className="wave-bars">
        {Array.from({ length: BANDS }, (_, i) => (
          <span key={i} className="wave-bar" style={{ height: `${Math.max(2, 18 * shaped * weights[i])}px`, opacity: active ? 1 : 0.35 }} />
        ))}
      </span>
    </div>
  );
}
