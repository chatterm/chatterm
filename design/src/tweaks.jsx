// Tweaks panel — tone + density.

const TweaksPanel = ({ open, tweaks, setTweaks }) => {
  if (!open) return null;
  const update = (key, val) => {
    const next = { ...tweaks, [key]: val };
    setTweaks(next);
    window.parent.postMessage({ type: "__edit_mode_set_keys", edits: { [key]: val } }, "*");
  };

  return (
    <div className="tweaks-panel open" style={{ display: "block" }}>
      <div style={{
        padding: "10px 12px", borderBottom: "1px solid var(--border-strong)",
        display: "flex", alignItems: "center", justifyContent: "space-between",
      }}>
        <div style={{ fontWeight: 600, color: "var(--text-strong)" }}>Tweaks</div>
        <span className="mono" style={{ fontSize: 10, color: "var(--text-mute)" }}>persisted to file</span>
      </div>
      <div style={{ padding: 12 }}>
        <TweakRow label="Theme tone">
          <Seg opts={[["neutral","Neutral"],["warm","Warm"],["cool","Cool"]]}
            value={tweaks.tone} onChange={(v) => update("tone", v)} />
        </TweakRow>
        <TweakRow label="Density">
          <Seg opts={[["compact","Compact"],["comfortable","Comfortable"]]}
            value={tweaks.density} onChange={(v) => update("density", v)} />
        </TweakRow>
      </div>
    </div>
  );
};

const TweakRow = ({ label, children }) => (
  <div style={{ marginBottom: 10 }}>
    <div style={{ fontSize: 11, color: "var(--text-dim)", marginBottom: 4 }}>{label}</div>
    {children}
  </div>
);

const Seg = ({ opts, value, onChange }) => (
  <div style={{ display: "flex", background: "#2d2d30", borderRadius: 4, padding: 2, gap: 2 }}>
    {opts.map(([k, label]) => (
      <button key={k} onClick={() => onChange(k)} style={{
        flex: 1, padding: "4px 6px", border: "none", borderRadius: 3, cursor: "pointer",
        background: value === k ? "var(--accent)" : "transparent",
        color: value === k ? "white" : "var(--text-dim)",
        fontSize: 11, fontFamily: "inherit", fontWeight: 500,
      }}>{label}</button>
    ))}
  </div>
);

Object.assign(window, { TweaksPanel });
