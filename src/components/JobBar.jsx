import { useApp } from '../app/store.jsx';
import { num } from '../lib/format.js';

const PHASES = {
  scan: 'Scanning directories',
  ecosystems: 'Reading manifests',
  read: 'Reading files',
  resolve: 'Resolving dependencies',
  metrics: 'Computing metrics',
  health: 'Evaluating health rules',
  done: 'Finishing',
};

/** Sticky progress strip shown while an analysis runs. */
export default function JobBar() {
  const { job, cancel, project } = useApp();
  if (!job || job.projectId !== project?.id) return null;
  const determinate = job.total > 0;
  const pct = determinate ? Math.min(100, (job.done / job.total) * 100) : 0;
  return (
    <div className="jobbar" role="status" aria-live="polite">
      <span className="spinner" />
      <span style={{ minWidth: 220 }}>
        {PHASES[job.phase] || 'Analysing'}
        {job.done > 0 && <span className="faint"> · {num(job.done)}{determinate ? ` / ${num(job.total)}` : ''}</span>}
      </span>
      <span className={`bar ${determinate ? '' : 'indeterminate'}`}>
        <i style={{ width: `${pct}%` }} />
      </span>
      <button type="button" className="btn sm" onClick={cancel}>
        Cancel
      </button>
    </div>
  );
}
