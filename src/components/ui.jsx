import { createContext, useCallback, useContext, useEffect, useRef, useState } from 'react';
import Icon from './Icon.jsx';
import { errorMessage } from '../lib/api.js';
import { SEVERITY_LABEL } from '../lib/format.js';

export function Loading({ label = 'Loading…' }) {
  return (
    <div className="loading" role="status">
      <span className="spinner" /> {label}
    </div>
  );
}

export function Empty({ icon = 'info', title, children, action }) {
  return (
    <div className="empty">
      <div className="icon-wrap">
        <Icon name={icon} size={22} />
      </div>
      {title && <h3>{title}</h3>}
      {children && <p>{children}</p>}
      {action}
    </div>
  );
}

export function ErrorState({ error, onRetry }) {
  return (
    <div className="banner danger" role="alert">
      <Icon name="alert" />
      <div style={{ flex: 1 }}>
        <div style={{ fontWeight: 600 }}>Something went wrong</div>
        <div className="muted selectable">{errorMessage(error)}</div>
      </div>
      {onRetry && (
        <button type="button" className="btn sm" onClick={onRetry}>
          Retry
        </button>
      )}
    </div>
  );
}

/** Renders loading/error states for a useAsync result, otherwise children(data). */
export function Async({ state, children, label }) {
  if (state.error) return <ErrorState error={state.error} onRetry={state.reload} />;
  if (state.loading && state.data === undefined) return <Loading label={label} />;
  return children(state.data);
}

export function SeverityBadge({ severity }) {
  return <span className={`badge ${severity === 'info' ? '' : severity}`}>{SEVERITY_LABEL[severity] || severity}</span>;
}

export function ConfidenceBadge({ confidence }) {
  const label = { high: 'High confidence', medium: 'Medium confidence', low: 'Low confidence' }[confidence] || confidence;
  return (
    <span className="badge" title="How reliable the underlying analysis is for this finding">
      {label}
    </span>
  );
}

export function Stat({ label, value, note, onClick }) {
  return (
    <div className={`card ${onClick ? 'clickable' : ''}`} onClick={onClick} role={onClick ? 'button' : undefined} tabIndex={onClick ? 0 : undefined}>
      <div className="stat">
        <span className="label">{label}</span>
        <span className="value">{value}</span>
        {note && <span className="note">{note}</span>}
      </div>
    </div>
  );
}

export function Tabs({ tabs, value, onChange }) {
  return (
    <div className="tabs" role="tablist">
      {tabs.map((t) => (
        <button key={t.id} type="button" role="tab" aria-selected={value === t.id} onClick={() => onChange(t.id)}>
          {t.label}
          {t.count != null && <span className="faint xs"> {t.count}</span>}
        </button>
      ))}
    </div>
  );
}

export function Segmented({ options, value, onChange, label }) {
  return (
    <div className="seg" role="group" aria-label={label}>
      {options.map((o) => (
        <button key={o.value} type="button" aria-pressed={value === o.value} onClick={() => onChange(o.value)} title={o.title}>
          {o.label}
        </button>
      ))}
    </div>
  );
}

export function SearchInput({ value, onChange, placeholder = 'Search…', autoFocus, style }) {
  return (
    <label className="search" style={style}>
      <Icon name="search" size={14} />
      <input className="input" value={value} placeholder={placeholder} onChange={(e) => onChange(e.target.value)} autoFocus={autoFocus} aria-label={placeholder} />
    </label>
  );
}

export function BarList({ items, max, onClick, format = (v) => v }) {
  const m = max ?? Math.max(1, ...items.map((i) => i.value));
  return (
    <div className="barlist">
      {items.map((i) => (
        <div key={i.key ?? i.label} className={`item ${onClick ? 'clickable' : ''}`} onClick={onClick ? () => onClick(i) : undefined}>
          <span className="name ellipsis" title={i.title || i.label}>
            {i.label}
          </span>
          <span className="faint num">{format(i.value)}</span>
          <span className="track">
            <i style={{ width: `${Math.max(1.5, (i.value / m) * 100)}%` }} />
          </span>
        </div>
      ))}
    </div>
  );
}

const DONUT_SHADES = [1, 0.72, 0.52, 0.38, 0.27, 0.19, 0.13, 0.08];

/** Monochrome donut: share is encoded by opacity steps, labelled in the legend. */
export function Donut({ items, size = 132, onClick }) {
  const total = items.reduce((s, i) => s + i.value, 0) || 1;
  const r = 52;
  const c = 2 * Math.PI * r;
  let offset = 0;
  return (
    <div className="row" style={{ gap: 20, alignItems: 'center' }}>
      <svg width={size} height={size} viewBox="0 0 132 132" role="img" aria-label="Distribution chart">
        <circle cx="66" cy="66" r={r} fill="none" stroke="var(--surface-3)" strokeWidth="16" />
        {items.map((i, idx) => {
          const len = (i.value / total) * c;
          const el = (
            <circle
              key={i.label}
              cx="66"
              cy="66"
              r={r}
              fill="none"
              stroke="var(--text)"
              strokeOpacity={DONUT_SHADES[idx] ?? 0.06}
              strokeWidth="16"
              strokeDasharray={`${Math.max(0, len - 1.5)} ${c}`}
              strokeDashoffset={-offset}
              transform="rotate(-90 66 66)"
            >
              <title>{`${i.label}: ${i.value}`}</title>
            </circle>
          );
          offset += len;
          return el;
        })}
      </svg>
      <div className="donut-legend">
        {items.map((i, idx) => (
          <div key={i.label} className={`row ${onClick ? 'clickable' : ''}`} style={{ cursor: onClick ? 'pointer' : 'default' }} onClick={onClick ? () => onClick(i) : undefined}>
            <span className="legend-swatch" style={{ background: 'var(--text)', opacity: DONUT_SHADES[idx] ?? 0.06 }} />
            <span>{i.label}</span>
            <span className="faint num">{Math.round((i.value / total) * 100)}%</span>
          </div>
        ))}
      </div>
    </div>
  );
}

export function Modal({ title, onClose, children, footer, wide }) {
  const ref = useRef(null);
  useEffect(() => {
    const onKey = (e) => e.key === 'Escape' && onClose?.();
    window.addEventListener('keydown', onKey);
    ref.current?.querySelector('input, textarea, select, button')?.focus();
    return () => window.removeEventListener('keydown', onKey);
  }, [onClose]);
  return (
    <div className="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && onClose?.()}>
      <div className={`modal ${wide ? 'wide' : ''}`} role="dialog" aria-modal="true" aria-label={title} ref={ref}>
        <div className="modal-head">
          <h2>{title}</h2>
          <button type="button" className="btn ghost icon" onClick={onClose} aria-label="Close">
            <Icon name="x" />
          </button>
        </div>
        <div className="modal-body">{children}</div>
        {footer && <div className="modal-foot">{footer}</div>}
      </div>
    </div>
  );
}

const ToastCtx = createContext(() => {});
export function ToastProvider({ children }) {
  const [toasts, setToasts] = useState([]);
  const push = useCallback((msg, kind = 'info') => {
    const id = Math.random().toString(36).slice(2);
    setToasts((t) => [...t, { id, msg, kind }]);
    setTimeout(() => setToasts((t) => t.filter((x) => x.id !== id)), kind === 'error' ? 7000 : 3500);
  }, []);
  return (
    <ToastCtx.Provider value={push}>
      {children}
      <div className="toasts" aria-live="polite">
        {toasts.map((t) => (
          <div key={t.id} className={`toast ${t.kind}`}>
            <Icon name={t.kind === 'error' ? 'alert' : 'check'} />
            <span className="selectable">{t.msg}</span>
          </div>
        ))}
      </div>
    </ToastCtx.Provider>
  );
}
export const useToast = () => useContext(ToastCtx);

export function Field({ label, help, children }) {
  return (
    <label className="field">
      <span>{label}</span>
      {children}
      {help && <span className="help">{help}</span>}
    </label>
  );
}

export function Check({ checked, onChange, children }) {
  return (
    <label className="check">
      <input type="checkbox" checked={!!checked} onChange={(e) => onChange(e.target.checked)} />
      {children}
    </label>
  );
}
