interface LevelMeterProps {
  label: string;
  level: number;
  active: boolean;
}

export function LevelMeter({ label, level, active }: LevelMeterProps) {
  const pct = Math.min(100, Math.round(level * 140));
  return (
    <div className="meter">
      <div className="meter-head">
        <span className={`dot ${active ? "dot-ok" : "dot-off"}`} />
        <span className="k">{label}</span>
      </div>
      <div className="meter-track">
        <div className="meter-fill" style={{ width: `${pct}%` }} />
      </div>
    </div>
  );
}
