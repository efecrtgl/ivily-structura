import { useState } from 'react';
import { useApp } from '../app/store.jsx';
import { api, errorMessage } from '../lib/api.js';
import { useAsync } from '../lib/hooks.js';
import { date, duration, relative } from '../lib/format.js';
import { confirm } from '../lib/dialogs.js';
import { Async, Empty, Segmented } from '../components/ui.jsx';
import Icon from '../components/Icon.jsx';

const KINDS = [
  { value: '', label: 'All' },
  { value: 'analysis', label: 'Analyses' },
  { value: 'snapshot', label: 'Snapshots' },
  { value: 'config', label: 'Configuration' },
  { value: 'report', label: 'Reports' },
  { value: 'data', label: 'Data' },
];

export default function History() {
  const { project, settings, toast } = useApp();
  const [kind, setKind] = useState('');
  const [onlyProject, setOnlyProject] = useState(!!project);
  const list = useAsync(() => api.historyList({ kind: kind || null, projectId: onlyProject && project ? project.id : null, limit: 500 }), [kind, onlyProject, project?.id]);
  const clear = async () => {
    if (!(await confirm('Clear the entire activity history? Projects, snapshots and mappings are not affected.'))) return;
    api.historyClear().then(() => (toast('History cleared'), list.reload()), (e) => toast(errorMessage(e), 'error'));
  };
  return (
    <div className="page">
      <div className="page-header">
        <div>
          <h1>History</h1>
          <div className="sub">
            Analyses, snapshots, comparisons and configuration changes. Entries keep counts only — never file contents. Retention: {settings?.historyRetentionDays ? `${settings.historyRetentionDays} days` : 'forever'}.
          </div>
        </div>
        <div className="actions">
          <button type="button" className="btn danger" onClick={clear}><Icon name="trash" size={14} /> Clear history</button>
        </div>
      </div>
      <div className="toolbar">
        <Segmented value={kind} onChange={setKind} options={KINDS} label="Kind" />
        {project && <Segmented value={onlyProject ? 'p' : 'all'} onChange={(v) => setOnlyProject(v === 'p')} options={[{ value: 'p', label: project.name }, { value: 'all', label: 'All projects' }]} label="Scope" />}
      </div>
      <Async state={list}>
        {(rows) =>
          rows.length === 0 ? (
            <div className="card"><Empty icon="history" title="No activity yet" /></div>
          ) : (
            <div className="card flush table-wrap">
              <table className="data">
                <thead><tr><th>When</th><th>Activity</th><th>Project</th><th>Profile</th><th>Outcome</th><th className="r">Duration</th></tr></thead>
                <tbody>
                  {rows.map((h) => (
                    <tr key={h.id}>
                      <td className="faint xs" title={date(h.ts)}>{relative(h.ts)}</td>
                      <td>
                        <div>{h.title}</div>
                        <div className="mono faint xs">{h.kind}{h.detail && Object.keys(h.detail).length ? ` · ${Object.entries(h.detail).map(([k, v]) => `${k}: ${v}`).join(', ')}` : ''}</div>
                      </td>
                      <td>{h.projectName || '—'}</td>
                      <td className="faint">{h.profileId || '—'}</td>
                      <td><span className={`badge ${h.outcome === 'success' ? 'ok' : h.outcome === 'failure' ? 'high' : 'medium'}`}>{h.outcome}</span></td>
                      <td className="r num faint">{h.durationMs != null ? duration(h.durationMs) : '—'}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )
        }
      </Async>
    </div>
  );
}
