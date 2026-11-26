import { useState } from 'react';
import { useApp } from '../app/store.jsx';
import { api, errorMessage } from '../lib/api.js';
import { useAsync } from '../lib/hooks.js';
import { bytes, date, num, relative, signed } from '../lib/format.js';
import { confirm, pickFile, pickSave, safeFileName } from '../lib/dialogs.js';
import { Async, Empty, Field, Modal, SeverityBadge } from '../components/ui.jsx';
import Icon from '../components/Icon.jsx';

export default function Snapshots({ params }) {
  const { project, analysis, setAnalysis, navigate, toast } = useApp();
  const list = useAsync(() => api.snapshotList(project.id), [project.id]);
  const projects = useAsync(() => api.projectList(), []);
  const [creating, setCreating] = useState(false);
  const [editing, setEditing] = useState(null);
  const [before, setBefore] = useState(params?.select ? { kind: 'snapshot', id: params.select } : null);
  const [after, setAfter] = useState(analysis ? { kind: 'analysis', id: analysis.id } : null);
  const [comparison, setComparison] = useState(null);
  const [busy, setBusy] = useState(false);

  const refOptions = [
    ...(analysis ? [{ key: `a:${analysis.id}`, ref: { kind: 'analysis', id: analysis.id }, label: analysis.source === 'snapshot' ? 'Opened snapshot (current view)' : 'Current analysis' }] : []),
    ...(list.data || []).map((s) => ({ key: `s:${s.id}`, ref: { kind: 'snapshot', id: s.id }, label: `${s.name} · ${date(s.createdAt)}` })),
  ];
  const keyOf = (r) => (r ? `${r.kind === 'analysis' ? 'a' : 's'}:${r.id}` : '');
  const fromKey = (k) => refOptions.find((o) => o.key === k)?.ref || null;

  const runCompare = async (b = before, a = after) => {
    if (!b || !a) return;
    setBusy(true);
    try {
      setComparison(await api.compare(b, a));
    } catch (e) {
      toast(errorMessage(e), 'error');
    } finally {
      setBusy(false);
    }
  };
  const compareProject = async (otherId) => {
    if (!otherId || !analysis) return;
    try {
      const r = await api.projectGet(Number(otherId));
      if (!r.analysis) return toast('That project has not been analysed yet', 'error');
      const b = { kind: 'analysis', id: r.analysis.id };
      const a = { kind: 'analysis', id: analysis.id };
      setComparison(await api.compare(b, a));
    } catch (e) {
      toast(errorMessage(e), 'error');
    }
  };

  const open = async (s) => {
    try {
      const h = await api.snapshotOpen(s.id);
      setAnalysis(h);
      toast(`Viewing snapshot "${s.name}" — re-analyse to return to live results`);
      navigate('dashboard');
    } catch (e) {
      toast(errorMessage(e), 'error');
    }
  };
  const exp = async (s) => {
    const dest = await pickSave('Export snapshot', `${safeFileName(project.name)}-${safeFileName(s.name)}.structura.json`, ['json'], 'Snapshot');
    if (!dest) return;
    api.snapshotExport(s.id, dest).then(() => toast('Snapshot exported'), (e) => toast(errorMessage(e), 'error'));
  };
  const imp = async () => {
    const src = await pickFile('Import snapshot', ['json'], 'Snapshot');
    if (!src) return;
    api.snapshotImport(project.id, src).then(() => (toast('Snapshot imported'), list.reload()), (e) => toast(errorMessage(e), 'error'));
  };
  const del = async (s) => {
    if (!(await confirm(`Delete snapshot "${s.name}"? This cannot be undone.`))) return;
    api.snapshotDelete(s.id).then(list.reload, (e) => toast(errorMessage(e), 'error'));
  };

  return (
    <div className="page">
      <div className="page-header">
        <div>
          <h1>Snapshots & compare</h1>
          <div className="sub">Snapshots store structure, metrics, graph and findings — never file contents. They never modify the project.</div>
        </div>
        <div className="actions">
          <button type="button" className="btn" onClick={imp}>
            <Icon name="upload" size={14} /> Import
          </button>
          <button type="button" className="btn primary" disabled={!analysis} onClick={() => setCreating(true)}>
            <Icon name="snapshot" size={14} /> Save snapshot
          </button>
        </div>
      </div>

      <div className="grid" style={{ gridTemplateColumns: 'minmax(320px, 420px) minmax(0, 1fr)' }}>
        <div className="card flush">
          <div className="card-head" style={{ padding: '14px 16px 0' }}>
            <h3>History</h3>
            <span className="hint">{list.data?.length || 0} snapshots</span>
          </div>
          <Async state={list}>
            {(items) =>
              items.length === 0 ? (
                <Empty icon="snapshot" title="No snapshots">
                  Save a snapshot after each analysis you want to keep, then compare snapshots to see how the structure evolves.
                </Empty>
              ) : (
                <div className="list" style={{ padding: '4px 10px 10px' }}>
                  {items.map((s) => (
                    <div key={s.id} className="li" style={{ alignItems: 'flex-start' }}>
                      <Icon name="snapshot" />
                      <div style={{ flex: 1, minWidth: 0 }}>
                        <div style={{ fontWeight: 500 }} className="ellipsis">{s.name}</div>
                        <div className="faint xs">
                          {date(s.createdAt)} · {num(s.summary.files)} files · {s.summary.cycles} cycles · {bytes(s.sizeBytes)}
                        </div>
                        {s.note && <div className="muted xs" style={{ marginTop: 2 }}>{s.note}</div>}
                        <div className="row" style={{ marginTop: 6, gap: 4 }}>
                          <button type="button" className="btn sm" onClick={() => open(s)} title="Restore this analysis into the views">Open</button>
                          <button type="button" className="btn sm" onClick={() => { const b = { kind: 'snapshot', id: s.id }; setBefore(b); runCompare(b, after); }}>Compare</button>
                          <button type="button" className="btn ghost sm icon" title="Rename / note" onClick={() => setEditing(s)}><Icon name="edit" size={13} /></button>
                          <button type="button" className="btn ghost sm icon" title="Export" onClick={() => exp(s)}><Icon name="download" size={13} /></button>
                          <button type="button" className="btn ghost sm icon danger" title="Delete" onClick={() => del(s)}><Icon name="trash" size={13} /></button>
                        </div>
                      </div>
                    </div>
                  ))}
                </div>
              )
            }
          </Async>
        </div>

        <div className="stack">
          <div className="card">
            <div className="card-head">
              <h3>Compare</h3>
            </div>
            <div className="row wrap">
              <select className="select" value={keyOf(before)} onChange={(e) => setBefore(fromKey(e.target.value))} aria-label="Before">
                <option value="">Before…</option>
                {refOptions.map((o) => <option key={o.key} value={o.key}>{o.label}</option>)}
              </select>
              <span className="faint">→</span>
              <select className="select" value={keyOf(after)} onChange={(e) => setAfter(fromKey(e.target.value))} aria-label="After">
                <option value="">After…</option>
                {refOptions.map((o) => <option key={o.key} value={o.key}>{o.label}</option>)}
              </select>
              <button type="button" className="btn primary" disabled={!before || !after || busy} onClick={() => runCompare()}>
                <Icon name="compare" size={14} /> Compare
              </button>
              {analysis && (
                <select className="select" value="" onChange={(e) => compareProject(e.target.value)} aria-label="Compare with another project">
                  <option value="">…or compare with another project</option>
                  {(projects.data || []).filter((p) => p.id !== project.id && p.lastAnalyzedAt).map((p) => <option key={p.id} value={p.id}>{p.name}</option>)}
                </select>
              )}
            </div>
          </div>
          {comparison ? <ComparisonView c={comparison} navigate={navigate} /> : <div className="card"><Empty icon="compare" title="Pick two analyses">Compare snapshots of this project over time, the current analysis against a snapshot, or two different projects.</Empty></div>}
        </div>
      </div>

      {creating && <CreateSnapshot onClose={() => setCreating(false)} onDone={() => { setCreating(false); list.reload(); }} />}
      {editing && <EditSnapshot s={editing} onClose={() => setEditing(null)} onDone={() => { setEditing(null); list.reload(); }} />}
    </div>
  );
}

function CreateSnapshot({ onClose, onDone }) {
  const { analysis, toast } = useApp();
  const [name, setName] = useState(`Snapshot ${new Date().toLocaleDateString('en-GB')}`);
  const [note, setNote] = useState('');
  const save = () => api.snapshotCreate(analysis.id, name, note).then(() => (toast('Snapshot saved'), onDone()), (e) => toast(errorMessage(e), 'error'));
  return (
    <Modal title="Save snapshot" onClose={onClose} footer={<><button type="button" className="btn" onClick={onClose}>Cancel</button><button type="button" className="btn primary" onClick={save} disabled={!name.trim()}>Save</button></>}>
      <Field label="Name"><input className="input" value={name} maxLength={120} onChange={(e) => setName(e.target.value)} /></Field>
      <Field label="Note" help="Why this snapshot matters (release, refactoring milestone, …)."><textarea className="textarea" value={note} onChange={(e) => setNote(e.target.value)} /></Field>
      <div className="faint xs">Analysed {relative(analysis.meta.startedAt)} with {analysis.meta.profileName}. {num(analysis.summary.files)} files.</div>
    </Modal>
  );
}

function EditSnapshot({ s, onClose, onDone }) {
  const { toast } = useApp();
  const [name, setName] = useState(s.name);
  const [note, setNote] = useState(s.note);
  const save = () => api.snapshotUpdate(s.id, name, note).then(onDone, (e) => toast(errorMessage(e), 'error'));
  return (
    <Modal title="Edit snapshot" onClose={onClose} footer={<><button type="button" className="btn" onClick={onClose}>Cancel</button><button type="button" className="btn primary" onClick={save} disabled={!name.trim()}>Save</button></>}>
      <Field label="Name"><input className="input" value={name} maxLength={120} onChange={(e) => setName(e.target.value)} /></Field>
      <Field label="Note"><textarea className="textarea" value={note} onChange={(e) => setNote(e.target.value)} /></Field>
    </Modal>
  );
}

function DiffList({ title, diff, icon = 'file', navigate, clickable }) {
  if (!diff || (diff.addedCount === 0 && diff.removedCount === 0)) return null;
  return (
    <div className="card">
      <div className="card-head">
        <h3>{title}</h3>
        <span className="hint">+{num(diff.addedCount)} / −{num(diff.removedCount)}</span>
      </div>
      <div className="grid g2">
        {[['Added', diff.added, diff.addedCount, 'ok'], ['Removed', diff.removed, diff.removedCount, 'high']].map(([label, items, count, cls]) => (
          <div key={label}>
            <div className="section-title">{label} ({num(count)})</div>
            <div className="list" style={{ maxHeight: 220, overflow: 'auto' }}>
              {items.slice(0, 200).map((x) => (
                <div key={x} className={`li ${clickable && label === 'Added' ? 'clickable' : ''}`} onClick={clickable && label === 'Added' ? () => navigate('explorer', { file: x }) : undefined}>
                  <span className={`dot ${cls === 'ok' ? '' : 'high'}`} style={cls === 'ok' ? { background: 'var(--ok)' } : undefined} />
                  <Icon name={icon} size={12} />
                  <span className="path ellipsis">{x}</span>
                </div>
              ))}
              {items.length === 0 && <span className="faint xs">none</span>}
              {count > items.length && <span className="faint xs">+{num(count - items.length)} more (see export)</span>}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

export function ComparisonView({ c, navigate }) {
  return (
    <div className="stack">
      <div className="card flush">
        <div className="card-head" style={{ padding: '14px 16px 0' }}>
          <h3>{c.beforeLabel} → {c.afterLabel}</h3>
          <span className="hint">{c.sameProject ? 'same project' : 'different projects'}</span>
        </div>
        {c.engineVersions[0] !== c.engineVersions[1] && <div className="banner info xs" style={{ margin: 12 }}>The two analyses were produced by different engine versions; small differences may come from improved analysis.</div>}
        <div className="table-wrap">
          <table className="data">
            <thead>
              <tr><th>Metric</th><th className="r">Before</th><th className="r">After</th><th className="r">Change</th><th>Kind</th></tr>
            </thead>
            <tbody>
              {c.metrics.map((m) => (
                <tr key={m.name}>
                  <td>{m.name}</td>
                  <td className="r num">{m.name.startsWith('Size') ? bytes(m.before) : num(Math.round(m.before * 10) / 10)}</td>
                  <td className="r num">{m.name.startsWith('Size') ? bytes(m.after) : num(Math.round(m.after * 10) / 10)}</td>
                  <td className="r num" style={{ fontWeight: m.change ? 600 : 400 }}>{signed(m.change, m.kind === 'estimated' ? 1 : 0)}</td>
                  <td className="faint xs">{m.kind}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>

      <div className="grid g2">
        <div className="card">
          <div className="card-head"><h3>Languages</h3></div>
          <div className="list">
            {c.languages.slice(0, 10).map((l) => (
              <div key={l.id} className="li">
                <span style={{ flex: 1 }}>{l.name}</span>
                <span className="faint xs num">{l.filesBefore} → {l.filesAfter} files</span>
                <span className="num small" style={{ width: 60, textAlign: 'right' }}>{signed(l.filesAfter - l.filesBefore)}</span>
              </div>
            ))}
          </div>
        </div>
        <div className="card">
          <div className="card-head"><h3>Findings & cycles</h3></div>
          <dl className="kv">
            <dt>New cycles</dt><dd>{c.cyclesNew.length}</dd>
            <dt>Resolved cycles</dt><dd>{c.cyclesResolved.length}</dd>
            <dt>New findings</dt><dd>{c.findingsNew.length}</dd>
            <dt>Resolved findings</dt><dd>{c.findingsResolved.length}</dd>
            <dt>Changed files</dt><dd>{num(c.changedFilesCount)}</dd>
          </dl>
          <div className="list" style={{ marginTop: 8 }}>
            {c.findingsNew.slice(0, 6).map((f, i) => (
              <div key={`n${i}`} className="li"><span className="badge">new</span><SeverityBadge severity={f.severity} /><span className="ellipsis small">{f.title}</span></div>
            ))}
            {c.findingsResolved.slice(0, 6).map((f, i) => (
              <div key={`r${i}`} className="li"><span className="badge ok">resolved</span><span className="ellipsis small">{f.title}</span></div>
            ))}
          </div>
        </div>
      </div>
      <DiffList title="Modules (top-level source folders)" diff={c.modules} icon="folder" />
      <DiffList title="Files" diff={c.files} navigate={navigate} clickable />
      <DiffList title="Directories" diff={c.directories} icon="folder" />
      <DiffList title="External packages" diff={c.packages} icon="package" />
      <DiffList title="Frameworks" diff={c.frameworks} icon="layers" />
      <DiffList title="Internal dependency edges" diff={c.dependencyEdges} icon="link" />
      {c.cyclesNew.length > 0 && (
        <div className="card">
          <div className="card-head"><h3>New cycles</h3></div>
          {c.cyclesNew.map((cy, i) => <div key={i} className="path" style={{ marginBottom: 6 }}>{cy.join(' ↔ ')}</div>)}
        </div>
      )}
    </div>
  );
}
