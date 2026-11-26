import { useEffect, useState } from 'react';
import { useApp } from '../app/store.jsx';
import { api, errorMessage } from '../lib/api.js';
import { useAsync, useDebounced } from '../lib/hooks.js';
import { pickSave, safeFileName } from '../lib/dialogs.js';
import { Check, ErrorState, Field, Loading, Segmented } from '../components/ui.jsx';
import Icon from '../components/Icon.jsx';

const SECTION_LABEL = {
  overview: 'Overview',
  structure: 'Project structure',
  languages: 'Language statistics',
  metrics: 'Metrics',
  dependencies: 'Dependencies',
  cycles: 'Circular dependencies',
  health: 'Architecture health',
  documentation: 'Documentation',
  notes: 'Notes & mapping',
};
const EXT = { markdown: ['md'], json: ['json'], csv: ['csv'] };

export default function Reports() {
  const { analysis, project, info, toast } = useApp();
  const snapshots = useAsync(() => api.snapshotList(project.id), [project.id]);
  const [format, setFormat] = useState('markdown');
  const [sections, setSections] = useState(info?.reportSections || []);
  const [csvTable, setCsvTable] = useState('files');
  const [maxItems, setMaxItems] = useState(30);
  const [title, setTitle] = useState('');
  const [compareWith, setCompareWith] = useState('');
  const [preview, setPreview] = useState(null);
  const [error, setError] = useState(null);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    api.profileList().then((ps) => {
      const p = ps.find((x) => x.id === analysis.meta.profileId);
      if (p?.reportSections?.length) setSections(p.reportSections);
    });
  }, [analysis.meta.profileId]);

  const request = {
    analysisId: analysis.id,
    options: { format, sections, csvTable, maxItems, title: title.trim() || null },
    compareWith: compareWith ? { kind: 'snapshot', id: Number(compareWith) } : null,
  };
  const key = useDebounced(JSON.stringify(request), 300);
  useEffect(() => {
    let alive = true;
    setLoading(true);
    api
      .reportPreview(JSON.parse(key))
      .then((s) => alive && (setPreview(s), setError(null)))
      .catch((e) => alive && setError(e))
      .finally(() => alive && setLoading(false));
    return () => {
      alive = false;
    };
  }, [key]);

  const toggle = (s) => setSections((v) => (v.includes(s) ? v.filter((x) => x !== s) : [...v, s]));
  const doExport = async () => {
    const name = `${safeFileName(project.name)}-${format === 'csv' ? csvTable : 'structure-report'}.${EXT[format][0]}`;
    const dest = await pickSave('Export report', name, EXT[format], format.toUpperCase());
    if (!dest) return;
    api.reportExport(request, dest).then(() => toast('Report exported'), (e) => toast(errorMessage(e), 'error'));
  };

  return (
    <div className="page full">
      <div className="page-header">
        <div>
          <h1>Reports</h1>
          <div className="sub">Exportable, configurable reports. They contain structure, metrics and findings — never source code.</div>
        </div>
        <div className="actions">
          <button type="button" className="btn primary" onClick={doExport} disabled={!!error || (format !== 'csv' && sections.length === 0)}>
            <Icon name="download" size={14} /> Export {format === 'markdown' ? 'Markdown' : format.toUpperCase()}
          </button>
        </div>
      </div>
      <div className="split">
        <div className="pane card">
          <div className="stack">
            <Field label="Format">
              <Segmented value={format} onChange={setFormat} options={[{ value: 'markdown', label: 'Markdown' }, { value: 'json', label: 'JSON' }, { value: 'csv', label: 'CSV' }]} />
            </Field>
            {format === 'csv' ? (
              <Field label="Table" help="CSV exports one table at a time. Cells are protected against spreadsheet formula injection.">
                <select className="select" value={csvTable} onChange={(e) => setCsvTable(e.target.value)}>
                  {info?.csvTables.map((t) => <option key={t} value={t}>{t}</option>)}
                </select>
              </Field>
            ) : (
              <>
                <div className="field">
                  <span>Sections</span>
                  {info?.reportSections.map((s) => (
                    <Check key={s} checked={sections.includes(s)} onChange={() => toggle(s)}>
                      {SECTION_LABEL[s] || s}
                    </Check>
                  ))}
                </div>
                {format === 'markdown' && (
                  <>
                    <Field label="Title"><input className="input" placeholder={`${project.name} — Structure Report`} value={title} onChange={(e) => setTitle(e.target.value)} /></Field>
                    <Field label="Rows per list"><input className="input" type="number" min={5} max={1000} value={maxItems} onChange={(e) => setMaxItems(Math.max(5, Math.min(1000, Number(e.target.value) || 30)))} /></Field>
                  </>
                )}
                <Field label="Include comparison with snapshot">
                  <select className="select" value={compareWith} onChange={(e) => setCompareWith(e.target.value)}>
                    <option value="">None</option>
                    {(snapshots.data || []).map((s) => <option key={s.id} value={s.id}>{s.name}</option>)}
                  </select>
                </Field>
              </>
            )}
          </div>
        </div>
        <div className="pane card flush">
          <div className="row between" style={{ padding: '10px 14px', borderBottom: '1px solid var(--border)' }}>
            <span className="small" style={{ fontWeight: 600 }}>Preview</span>
            {loading && <span className="spinner" />}
          </div>
          <div className="pane-body">
            {error ? <div style={{ padding: 16 }}><ErrorState error={error} /></div> : preview == null ? <Loading /> : <pre className="md-preview selectable">{preview}</pre>}
          </div>
        </div>
      </div>
    </div>
  );
}
