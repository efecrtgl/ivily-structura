import { useEffect, useState } from 'react';
import { useApp } from '../app/store.jsx';
import { api, errorMessage } from '../lib/api.js';
import { useAsync } from '../lib/hooks.js';
import { matchesAny } from '../lib/glob.js';
import { relative } from '../lib/format.js';
import { confirm, pickFile, pickSave, safeFileName } from '../lib/dialogs.js';
import { Async, Check, Empty, Field, Modal, Tabs } from '../components/ui.jsx';
import Icon from '../components/Icon.jsx';

const RELATION_KINDS = ['depends-on', 'calls', 'owns', 'replaces', 'related', 'deploys-with'];
const splitPatterns = (s) => s.split(/[\n,]/).map((x) => x.trim()).filter(Boolean);

export default function Mapping() {
  const { project, analysis, toast } = useApp();
  const mapping = useAsync(() => api.mappingGet(project.id), [project.id]);
  const modules = useAsync(async () => {
    if (!analysis) return [];
    const top = await api.tree(analysis.id, '');
    const dirs = top.filter((t) => t.isDir);
    const second = (await Promise.all(dirs.slice(0, 40).map((d) => api.tree(analysis.id, d.path)))).flat().filter((t) => t.isDir);
    return [...dirs, ...second];
  }, [analysis?.id]);
  const [tab, setTab] = useState('map');

  const run = (p, msg) =>
    p.then(
      () => {
        if (msg) toast(msg);
        mapping.reload();
        return true;
      },
      (e) => {
        toast(errorMessage(e), 'error');
        return false;
      },
    );

  const exportMapping = async () => {
    const dest = await pickSave('Export project mapping', `${safeFileName(project.name)}-mapping.json`, ['json'], 'Mapping');
    if (dest) run(api.mappingExport(project.id, dest), 'Mapping exported');
  };
  const importMapping = async () => {
    const src = await pickFile('Import project mapping', ['json'], 'Mapping');
    if (!src) return;
    if (!(await confirm('Importing replaces this project\'s current mapping (annotations, groups, layers, relations and notes). Continue?'))) return;
    run(api.mappingImport(project.id, src), 'Mapping imported');
  };

  return (
    <div className="page">
      <div className="page-header">
        <div>
          <h1>Project mapping</h1>
          <div className="sub">Your own architecture model: labels, groups, layers, relations and notes. Independent of automated analysis and preserved across re-analysis.</div>
        </div>
        <div className="actions">
          <button type="button" className="btn" onClick={importMapping}><Icon name="upload" size={14} /> Import</button>
          <button type="button" className="btn" onClick={exportMapping}><Icon name="download" size={14} /> Export</button>
        </div>
      </div>
      <Async state={mapping}>
        {(m) => (
          <>
            <Tabs
              value={tab}
              onChange={setTab}
              tabs={[
                { id: 'map', label: 'Visual map' },
                { id: 'layers', label: 'Layers', count: m.layers.length },
                { id: 'groups', label: 'Module groups', count: m.groups.length },
                { id: 'annotations', label: 'Annotations', count: m.annotations.length },
                { id: 'relations', label: 'Relations', count: m.relations.length },
                { id: 'notes', label: 'Notes', count: m.notes.length },
              ]}
            />
            {tab === 'map' && <VisualMap m={m} modules={modules.data || []} />}
            {tab === 'layers' && <Layers m={m} onSave={(layers) => run(api.layersSet(project.id, layers), 'Layers saved — they are checked on the next analysis')} />}
            {tab === 'groups' && <Groups m={m} run={run} projectId={project.id} />}
            {tab === 'annotations' && <Annotations m={m} run={run} projectId={project.id} />}
            {tab === 'relations' && <Relations m={m} run={run} projectId={project.id} />}
            {tab === 'notes' && <Notes m={m} run={run} projectId={project.id} />}
          </>
        )}
      </Async>
    </div>
  );
}

function VisualMap({ m, modules }) {
  const { navigate } = useApp();
  if (!m.layers.length && !m.groups.length) {
    return (
      <div className="card">
        <Empty icon="map" title="No map yet">
          Define layers (e.g. UI → Services → Core) and module groups. The map shows which folders fall into each layer, and Structura flags imports that go against your layer order.
        </Empty>
      </div>
    );
  }
  const assigned = new Set();
  const groupOf = (path) => m.groups.find((g) => matchesAny(path, g.patterns));
  const annotation = (path) => m.annotations.find((a) => a.path === path);
  return (
    <div className="stack">
      {m.layers.map((l, i) => {
        const mods = modules.filter((d) => matchesAny(d.path, l.patterns) && !assigned.has(d.path) && !modules.some((p) => p.path !== d.path && d.path.startsWith(`${p.path}/`) && matchesAny(p.path, l.patterns)));
        mods.forEach((d) => assigned.add(d.path));
        return (
          <div key={l.id} className="card">
            <div className="card-head">
              <h3>
                <span className="badge solid" style={{ marginRight: 8 }}>{i + 1}</span>
                {l.name}
              </h3>
              <span className="hint mono">{l.patterns.join(', ') || 'no patterns'}</span>
            </div>
            {l.description && <p className="muted small" style={{ marginBottom: 8 }}>{l.description}</p>}
            <div className="row wrap">
              {mods.map((d) => {
                const g = groupOf(d.path);
                const a = annotation(d.path);
                return (
                  <button key={d.path} type="button" className="chip" style={{ cursor: 'pointer', borderColor: g?.color || undefined }} onClick={() => navigate('explorer', { path: d.path })} title={a?.note || d.path}>
                    {a?.important && <Icon name="star" size={11} />}
                    <span className="mono">{d.path}</span>
                    {a?.label && <b>{a.label}</b>}
                    {g && <span className="faint">· {g.name}</span>}
                  </button>
                );
              })}
              {mods.length === 0 && <span className="faint small">No analysed folders match this layer's patterns.</span>}
            </div>
            {i < m.layers.length - 1 && <div className="faint xs" style={{ marginTop: 8 }}>↓ may depend on layers below</div>}
          </div>
        );
      })}
      {m.groups.length > 0 && (
        <div className="card">
          <div className="card-head"><h3>Module groups</h3></div>
          <div className="row wrap">
            {m.groups.map((g) => (
              <span key={g.id} className="chip" style={{ borderColor: g.color || undefined }}>
                <b>{g.name}</b> <span className="mono faint">{g.patterns.join(', ')}</span>
              </span>
            ))}
          </div>
        </div>
      )}
      {m.relations.length > 0 && (
        <div className="card">
          <div className="card-head"><h3>User-defined relations</h3></div>
          <div className="list">
            {m.relations.map((r) => (
              <div key={r.id} className="li">
                <span className="mono small">{r.fromPath}</span>
                <span className="badge">{r.kind}</span>
                <span className="mono small">{r.toPath}</span>
                <span className="faint xs ellipsis">{r.note}</span>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

function Layers({ m, onSave }) {
  const [layers, setLayers] = useState(m.layers.map((l) => ({ ...l, patternsText: l.patterns.join('\n') })));
  useEffect(() => setLayers(m.layers.map((l) => ({ ...l, patternsText: l.patterns.join('\n') }))), [m.layers]);
  const update = (i, patch) => setLayers((ls) => ls.map((l, k) => (k === i ? { ...l, ...patch } : l)));
  const move = (i, d) =>
    setLayers((ls) => {
      const c = [...ls];
      const [x] = c.splice(i, 1);
      c.splice(i + d, 0, x);
      return c;
    });
  return (
    <div className="stack">
      <div className="banner info small">
        <Icon name="info" /> List layers from top (e.g. UI) to bottom (e.g. core/data). A layer may import from layers <b>below</b> it. Imports from a lower layer to a higher one are reported as layer violations on the next analysis.
      </div>
      {layers.map((l, i) => (
        <div key={i} className="card">
          <div className="row" style={{ alignItems: 'flex-start', gap: 12 }}>
            <span className="badge solid" style={{ marginTop: 6 }}>{i + 1}</span>
            <div className="grid g2" style={{ flex: 1 }}>
              <Field label="Name"><input className="input" value={l.name} onChange={(e) => update(i, { name: e.target.value })} /></Field>
              <Field label="Patterns" help="One per line, e.g. src/ui or src/**/components"><textarea className="textarea" style={{ minHeight: 60 }} value={l.patternsText} onChange={(e) => update(i, { patternsText: e.target.value })} /></Field>
              <Field label="Description"><input className="input" value={l.description} onChange={(e) => update(i, { description: e.target.value })} /></Field>
            </div>
            <div className="stack" style={{ gap: 4 }}>
              <button type="button" className="btn sm icon" disabled={i === 0} onClick={() => move(i, -1)} aria-label="Move up">↑</button>
              <button type="button" className="btn sm icon" disabled={i === layers.length - 1} onClick={() => move(i, 1)} aria-label="Move down">↓</button>
              <button type="button" className="btn sm icon danger" onClick={() => setLayers((ls) => ls.filter((_, k) => k !== i))} aria-label="Remove"><Icon name="trash" size={12} /></button>
            </div>
          </div>
        </div>
      ))}
      <div className="row">
        <button type="button" className="btn" onClick={() => setLayers((ls) => [...ls, { name: '', description: '', patternsText: '', patterns: [] }])}><Icon name="plus" size={14} /> Add layer</button>
        <button type="button" className="btn primary" onClick={() => onSave(layers.map((l) => ({ id: 0, rank: 0, name: l.name, description: l.description, patterns: splitPatterns(l.patternsText) })))} disabled={layers.some((l) => !l.name.trim())}>
          Save layers
        </button>
      </div>
    </div>
  );
}

function Groups({ m, run, projectId }) {
  const [edit, setEdit] = useState(null);
  return (
    <div className="stack">
      <div className="row"><button type="button" className="btn" onClick={() => setEdit({ id: 0, name: '', color: '', description: '', patterns: [] })}><Icon name="plus" size={14} /> New group</button></div>
      {m.groups.length === 0 && <div className="card"><Empty icon="layers" title="No groups">Groups bundle related folders into logical modules (e.g. "Billing", "Auth").</Empty></div>}
      {m.groups.map((g) => (
        <div key={g.id} className="card row between">
          <div>
            <b>{g.name}</b> {g.color && <span className="legend-swatch" style={{ background: g.color, marginLeft: 6 }} />}
            <div className="mono faint xs">{g.patterns.join(', ') || 'assigned via annotations'}</div>
            {g.description && <div className="muted small">{g.description}</div>}
          </div>
          <div className="row">
            <button type="button" className="btn sm" onClick={() => setEdit(g)}>Edit</button>
            <button type="button" className="btn sm danger" onClick={async () => (await confirm(`Delete group "${g.name}"?`)) && run(api.groupDelete(projectId, g.id))}>Delete</button>
          </div>
        </div>
      ))}
      {edit && <GroupModal g={edit} onClose={() => setEdit(null)} onSave={(g) => run(api.groupSave(projectId, g), 'Group saved').then((ok) => ok && setEdit(null))} />}
    </div>
  );
}

function GroupModal({ g, onClose, onSave }) {
  const [v, setV] = useState({ ...g, patternsText: g.patterns.join('\n') });
  return (
    <Modal title={g.id ? 'Edit group' : 'New group'} onClose={onClose} footer={<><button type="button" className="btn" onClick={onClose}>Cancel</button><button type="button" className="btn primary" disabled={!v.name.trim()} onClick={() => onSave({ id: v.id, name: v.name, color: v.color, description: v.description, patterns: splitPatterns(v.patternsText) })}>Save</button></>}>
      <Field label="Name"><input className="input" value={v.name} onChange={(e) => setV({ ...v, name: e.target.value })} /></Field>
      <Field label="Patterns" help="Folders/globs belonging to this group, one per line."><textarea className="textarea" value={v.patternsText} onChange={(e) => setV({ ...v, patternsText: e.target.value })} /></Field>
      <Field label="Outline colour (optional)"><input className="input" type="color" value={v.color || '#888888'} onChange={(e) => setV({ ...v, color: e.target.value })} style={{ width: 60, padding: 2 }} /></Field>
      <Field label="Description"><textarea className="textarea" value={v.description} onChange={(e) => setV({ ...v, description: e.target.value })} /></Field>
    </Modal>
  );
}

function Annotations({ m, run, projectId }) {
  const [edit, setEdit] = useState(null);
  const [filter, setFilter] = useState('');
  const rows = m.annotations.filter((a) => !filter || a.path.includes(filter) || a.label.toLowerCase().includes(filter.toLowerCase()));
  return (
    <div className="stack">
      <div className="row">
        <button type="button" className="btn" onClick={() => setEdit({ path: '', label: '', note: '', important: false, entryPoint: false, groupId: null })}><Icon name="plus" size={14} /> Annotate a path</button>
        <input className="input" placeholder="Filter…" value={filter} onChange={(e) => setFilter(e.target.value)} />
      </div>
      {rows.length === 0 ? (
        <div className="card"><Empty icon="note" title="No annotations">Annotate files or folders here or from the Explorer's file panel.</Empty></div>
      ) : (
        <div className="card flush table-wrap">
          <table className="data">
            <thead><tr><th>Path</th><th>Label</th><th>Note</th><th>Flags</th><th>Group</th><th>Updated</th><th /></tr></thead>
            <tbody>
              {rows.map((a) => (
                <tr key={a.id}>
                  <td className="path">{a.path || '(root)'}</td>
                  <td>{a.label}</td>
                  <td className="muted small" style={{ maxWidth: 320 }}><div className="ellipsis">{a.note}</div></td>
                  <td>{a.important && <span className="badge">important</span>} {a.entryPoint && <span className="badge">entry</span>}</td>
                  <td>{m.groups.find((g) => g.id === a.groupId)?.name || '—'}</td>
                  <td className="faint xs">{relative(a.updatedAt)}</td>
                  <td><button type="button" className="btn sm" onClick={() => setEdit(a)}>Edit</button></td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      {edit && <AnnotationModal a={edit} groups={m.groups} onClose={() => setEdit(null)} onSave={(a) => run(api.annotationSet(projectId, a), 'Annotation saved').then((ok) => ok && setEdit(null))} />}
    </div>
  );
}

function AnnotationModal({ a, groups, onClose, onSave }) {
  const [v, setV] = useState(a);
  return (
    <Modal title="Annotation" onClose={onClose} footer={<><button type="button" className="btn" onClick={onClose}>Cancel</button><button type="button" className="btn primary" onClick={() => onSave(v)}>Save</button></>}>
      <Field label="Path" help="Project-relative, e.g. src/payments (empty = project root)."><input className="input mono" value={v.path} onChange={(e) => setV({ ...v, path: e.target.value })} /></Field>
      <Field label="Label"><input className="input" value={v.label} onChange={(e) => setV({ ...v, label: e.target.value })} /></Field>
      <Field label="Note"><textarea className="textarea" value={v.note} onChange={(e) => setV({ ...v, note: e.target.value })} /></Field>
      <Field label="Group">
        <select className="select" value={v.groupId || ''} onChange={(e) => setV({ ...v, groupId: e.target.value ? Number(e.target.value) : null })}>
          <option value="">None</option>
          {groups.map((g) => <option key={g.id} value={g.id}>{g.name}</option>)}
        </select>
      </Field>
      <div className="row" style={{ gap: 16 }}>
        <Check checked={v.important} onChange={(x) => setV({ ...v, important: x })}>Important</Check>
        <Check checked={v.entryPoint} onChange={(x) => setV({ ...v, entryPoint: x })}>Entry point</Check>
      </div>
      <div className="faint xs">Clearing all fields removes the annotation.</div>
    </Modal>
  );
}

function Relations({ m, run, projectId }) {
  const [v, setV] = useState({ fromPath: '', toPath: '', kind: 'depends-on', note: '' });
  return (
    <div className="stack">
      <div className="card">
        <div className="card-head"><h3>Add relation</h3><span className="hint">relations you know about that static analysis cannot see (runtime calls, deployment, ownership)</span></div>
        <div className="row wrap">
          <input className="input mono" placeholder="from (e.g. src/web)" value={v.fromPath} onChange={(e) => setV({ ...v, fromPath: e.target.value })} />
          <select className="select" value={v.kind} onChange={(e) => setV({ ...v, kind: e.target.value })}>{RELATION_KINDS.map((k) => <option key={k}>{k}</option>)}</select>
          <input className="input mono" placeholder="to (e.g. services/api)" value={v.toPath} onChange={(e) => setV({ ...v, toPath: e.target.value })} />
          <input className="input" placeholder="note" style={{ flex: 1 }} value={v.note} onChange={(e) => setV({ ...v, note: e.target.value })} />
          <button type="button" className="btn primary" disabled={!v.fromPath || !v.toPath} onClick={() => run(api.relationAdd(projectId, v), 'Relation added').then((ok) => ok && setV({ fromPath: '', toPath: '', kind: 'depends-on', note: '' }))}>Add</button>
        </div>
      </div>
      <div className="card flush">
        <div className="list" style={{ padding: 8 }}>
          {m.relations.length === 0 && <span className="faint small" style={{ padding: 8 }}>No relations.</span>}
          {m.relations.map((r) => (
            <div key={r.id} className="li">
              <span className="mono small">{r.fromPath}</span>
              <span className="badge">{r.kind}</span>
              <span className="mono small">{r.toPath}</span>
              <span className="faint xs ellipsis" style={{ flex: 1 }}>{r.note}</span>
              <button type="button" className="btn ghost sm icon" aria-label="Delete relation" onClick={() => run(api.relationDelete(projectId, r.id))}><Icon name="trash" size={13} /></button>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

function Notes({ m, run, projectId }) {
  const [edit, setEdit] = useState(null);
  return (
    <div className="stack">
      <div className="row"><button type="button" className="btn" onClick={() => setEdit({ id: 0, title: '', body: '' })}><Icon name="plus" size={14} /> New note</button></div>
      {m.notes.length === 0 && <div className="card"><Empty icon="note" title="No architecture notes">Record decisions, conventions and known problems. Notes can be included in reports.</Empty></div>}
      <div className="grid g2">
        {m.notes.map((n) => (
          <div key={n.id} className="card">
            <div className="card-head">
              <h3>{n.title}</h3>
              <div className="row">
                <button type="button" className="btn ghost sm" onClick={() => setEdit(n)}>Edit</button>
                <button type="button" className="btn ghost sm danger" onClick={async () => (await confirm(`Delete note "${n.title}"?`)) && run(api.noteDelete(projectId, n.id))}>Delete</button>
              </div>
            </div>
            <p className="muted small selectable" style={{ whiteSpace: 'pre-wrap' }}>{n.body}</p>
            <div className="faint xs" style={{ marginTop: 8 }}>updated {relative(n.updatedAt)}</div>
          </div>
        ))}
      </div>
      {edit && (
        <Modal title={edit.id ? 'Edit note' : 'New note'} wide onClose={() => setEdit(null)} footer={<><button type="button" className="btn" onClick={() => setEdit(null)}>Cancel</button><button type="button" className="btn primary" disabled={!edit.title.trim()} onClick={() => run(api.noteSave(projectId, edit), 'Note saved').then((ok) => ok && setEdit(null))}>Save</button></>}>
          <Field label="Title"><input className="input" value={edit.title} onChange={(e) => setEdit({ ...edit, title: e.target.value })} /></Field>
          <Field label="Body (Markdown)"><textarea className="textarea" style={{ minHeight: 240 }} value={edit.body} onChange={(e) => setEdit({ ...edit, body: e.target.value })} /></Field>
        </Modal>
      )}
    </div>
  );
}

