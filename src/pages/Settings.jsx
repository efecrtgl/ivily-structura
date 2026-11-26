import { useEffect, useState } from 'react';
import { relaunch } from '@tauri-apps/plugin-process';
import { useApp } from '../app/store.jsx';
import { api, errorMessage } from '../lib/api.js';
import { useAsync } from '../lib/hooks.js';
import { bytes, num } from '../lib/format.js';
import { confirm, pickFile, pickSave } from '../lib/dialogs.js';
import { Async, Check, Field, Modal, Segmented, Tabs } from '../components/ui.jsx';
import Icon from '../components/Icon.jsx';

export default function Settings() {
  const { project } = useApp();
  const [tab, setTab] = useState('general');
  const tabs = [
    { id: 'general', label: 'General' },
    { id: 'profiles', label: 'Analysis profiles' },
    ...(project ? [{ id: 'project', label: 'This project' }] : []),
    { id: 'data', label: 'Data & backup' },
    { id: 'updates', label: 'Updates' },
    { id: 'about', label: 'About & privacy' },
  ];
  return (
    <div className="page">
      <div className="page-header">
        <div>
          <h1>Settings</h1>
        </div>
      </div>
      <Tabs tabs={tabs} value={tab} onChange={setTab} />
      {tab === 'general' && <General />}
      {tab === 'profiles' && <Profiles />}
      {tab === 'project' && project && <ProjectSettings />}
      {tab === 'data' && <Data />}
      {tab === 'updates' && <Updates />}
      {tab === 'about' && <About />}
    </div>
  );
}

function General() {
  const { settings, saveSettings, toast } = useApp();
  const profiles = useAsync(() => api.profileList(), []);
  const set = (patch) => saveSettings({ ...settings, ...patch }).catch((e) => toast(errorMessage(e), 'error'));
  return (
    <div className="grid g2">
      <div className="card stack">
        <h3>Appearance</h3>
        <Field label="Theme">
          <Segmented value={settings.theme} onChange={(theme) => set({ theme })} options={[{ value: 'system', label: 'System' }, { value: 'dark', label: 'Dark' }, { value: 'light', label: 'Light' }]} />
        </Field>
        <Check checked={settings.reduceMotion} onChange={(reduceMotion) => set({ reduceMotion })}>Reduce motion</Check>
        <Check checked={settings.reopenLastProject} onChange={(reopenLastProject) => set({ reopenLastProject })}>Reopen the last project on start</Check>
        <Check checked={settings.confirmDestructive} onChange={(confirmDestructive) => set({ confirmDestructive })}>Confirm before deleting</Check>
      </div>
      <div className="card stack">
        <h3>Analysis & views</h3>
        <Field label="Default profile">
          <select className="select" value={settings.defaultProfile} onChange={(e) => set({ defaultProfile: e.target.value })}>
            {(profiles.data || []).map((p) => <option key={p.id} value={p.id}>{p.name}</option>)}
          </select>
        </Field>
        <Field label="Explorer tree depth limit">
          <input className="input" type="number" min={1} max={256} value={settings.explorerMaxDepth} onChange={(e) => set({ explorerMaxDepth: Number(e.target.value) || 32 })} />
        </Field>
        <Field label="Default graph node limit" help="Large graphs are reduced to their most connected nodes.">
          <input className="input" type="number" min={20} max={3000} value={settings.graphMaxNodes} onChange={(e) => set({ graphMaxNodes: Math.min(3000, Math.max(20, Number(e.target.value) || 300)) })} />
        </Field>
        <Field label="Default graph layout">
          <select className="select" value={settings.graphLayout} onChange={(e) => set({ graphLayout: e.target.value })}>
            {['cose', 'breadthfirst', 'concentric', 'circle', 'grid'].map((l) => <option key={l}>{l}</option>)}
          </select>
        </Field>
      </div>
    </div>
  );
}

function Profiles() {
  const { toast } = useApp();
  const list = useAsync(() => api.profileList(), []);
  const [edit, setEdit] = useState(null);
  const duplicate = (p) => setEdit({ ...structuredClone(p), id: `${p.id}-custom-${Date.now().toString(36)}`, name: `${p.name} (custom)`, builtin: false });
  const del = async (p) => {
    if (!(await confirm(`Delete profile "${p.name}"?`))) return;
    api.profileDelete(p.id).then(list.reload, (e) => toast(errorMessage(e), 'error'));
  };
  return (
    <Async state={list}>
      {(ps) => (
        <div className="stack">
          {ps.map((p) => (
            <div key={p.id} className="card row between" style={{ alignItems: 'flex-start' }}>
              <div style={{ maxWidth: 760 }}>
                <div className="row">
                  <b>{p.name}</b>
                  {p.builtin ? <span className="badge">built-in</span> : <span className="badge solid">custom</span>}
                </div>
                <div className="muted small" style={{ marginTop: 4 }}>{p.description}</div>
                <div className="row wrap" style={{ marginTop: 8 }}>
                  <span className="chip">{p.content ? 'reads files' : 'metadata only'}</span>
                  {p.dependencies && <span className="chip">dependency graph</span>}
                  {p.duplicates && <span className="chip">duplicates</span>}
                  {p.health && <span className="chip">health: {p.healthConfig.categories.length ? p.healthConfig.categories.join(', ') : 'all rules'}</span>}
                  <span className="chip">{p.scan.respectGitignore ? '.gitignore respected' : 'ignores .gitignore'}</span>
                  {p.scan.includeHidden && <span className="chip">hidden files</span>}
                  <span className="chip">{p.scan.exclude.length} exclusions</span>
                </div>
              </div>
              <div className="row">
                <button type="button" className="btn sm" onClick={() => duplicate(p)}>Duplicate</button>
                {!p.builtin && <button type="button" className="btn sm" onClick={() => setEdit(structuredClone(p))}>Edit</button>}
                {!p.builtin && <button type="button" className="btn sm danger" onClick={() => del(p)}>Delete</button>}
              </div>
            </div>
          ))}
          {edit && <ProfileEditor initial={edit} onClose={() => setEdit(null)} onSaved={() => (setEdit(null), list.reload())} />}
        </div>
      )}
    </Async>
  );
}

const CATS = ['dependencies', 'size', 'structure', 'documentation', 'testing', 'configuration', 'security'];
const THRESHOLDS = [
  ['largeFileLines', 'Large file (lines)'],
  ['hugeFileLines', 'Very large file (lines)'],
  ['complexFile', 'Complex file (decision points)'],
  ['deepBlockNesting', 'Deep block nesting'],
  ['highFanIn', 'High fan-in'],
  ['highFanOut', 'High fan-out'],
  ['deepDirectory', 'Deep directory (levels)'],
  ['wideDirectory', 'Crowded directory (files)'],
  ['duplicateMinBytes', 'Duplicate min. size (bytes)'],
  ['singleChildChain', 'Single-folder chain length'],
  ['minSourceFilesForTests', 'Min. source files for test check'],
  ['largeCycle', 'Large cycle (files)'],
];

function ProfileEditor({ initial, onClose, onSaved }) {
  const { info, toast } = useApp();
  const [p, setP] = useState(initial);
  const [excl, setExcl] = useState(initial.scan.exclude.join('\n'));
  const set = (patch) => setP((x) => ({ ...x, ...patch }));
  const setScan = (patch) => setP((x) => ({ ...x, scan: { ...x.scan, ...patch } }));
  const setHealth = (patch) => setP((x) => ({ ...x, healthConfig: { ...x.healthConfig, ...patch } }));
  const save = () => {
    const profile = { ...p, scan: { ...p.scan, exclude: excl.split('\n').map((s) => s.trim()).filter(Boolean) } };
    api.profileSave(profile).then(() => (toast('Profile saved'), onSaved()), (e) => toast(errorMessage(e), 'error'));
  };
  const toggleIn = (list, v) => (list.includes(v) ? list.filter((x) => x !== v) : [...list, v]);
  return (
    <Modal title="Edit analysis profile" wide onClose={onClose} footer={<><button type="button" className="btn" onClick={onClose}>Cancel</button><button type="button" className="btn primary" onClick={save}>Save profile</button></>}>
      <div className="grid g2">
        <Field label="Name"><input className="input" value={p.name} onChange={(e) => set({ name: e.target.value })} /></Field>
        <Field label="Id" help="Letters, digits, - and _"><input className="input mono" value={p.id} onChange={(e) => set({ id: e.target.value })} /></Field>
      </div>
      <Field label="Description"><textarea className="textarea" style={{ minHeight: 50 }} value={p.description} onChange={(e) => set({ description: e.target.value })} /></Field>
      <div className="section-title">Analyzers</div>
      <div className="row wrap" style={{ gap: 18 }}>
        <Check checked={p.content} onChange={(content) => set({ content, dependencies: content && p.dependencies })}>Read file contents (lines, symbols, complexity)</Check>
        <Check checked={p.dependencies} onChange={(dependencies) => set({ dependencies, content: p.content || dependencies })}>Dependency graph</Check>
        <Check checked={p.duplicates} onChange={(duplicates) => set({ duplicates })}>Duplicate detection</Check>
        <Check checked={p.health} onChange={(health) => set({ health })}>Health rules</Check>
      </div>
      <div className="section-title">Scan scope</div>
      <div className="grid g3">
        <Field label="Max depth (empty = unlimited)"><input className="input" type="number" min={1} value={p.scan.maxDepth ?? ''} onChange={(e) => setScan({ maxDepth: e.target.value ? Number(e.target.value) : null })} /></Field>
        <Field label="Max entries"><input className="input" type="number" min={1000} max={5000000} value={p.scan.maxEntries} onChange={(e) => setScan({ maxEntries: Number(e.target.value) || 250000 })} /></Field>
        <Field label="Max file size to read (KB)"><input className="input" type="number" min={1} max={65536} value={p.maxFileKb} onChange={(e) => set({ maxFileKb: Number(e.target.value) || 1024 })} /></Field>
      </div>
      <div className="row wrap" style={{ gap: 18 }}>
        <Check checked={p.scan.respectGitignore} onChange={(respectGitignore) => setScan({ respectGitignore })}>Respect .gitignore / .ignore</Check>
        <Check checked={p.scan.includeHidden} onChange={(includeHidden) => setScan({ includeHidden })}>Include hidden files</Check>
      </div>
      <Field label="Excluded paths" help="One glob per line. A bare name (node_modules) excludes that folder anywhere."><textarea className="textarea mono" style={{ minHeight: 120 }} value={excl} onChange={(e) => setExcl(e.target.value)} /></Field>
      {p.health && (
        <>
          <div className="section-title">Health rule categories (none selected = all)</div>
          <div className="row wrap" style={{ gap: 14 }}>
            {CATS.map((c) => <Check key={c} checked={p.healthConfig.categories.includes(c)} onChange={() => setHealth({ categories: toggleIn(p.healthConfig.categories, c) })}>{c}</Check>)}
          </div>
          <div className="section-title">Disabled rules</div>
          <div className="row wrap" style={{ gap: 14 }}>
            {info?.rules.map((r) => <Check key={r.id} checked={p.healthConfig.disabledRules.includes(r.id)} onChange={() => setHealth({ disabledRules: toggleIn(p.healthConfig.disabledRules, r.id) })}>{r.title}</Check>)}
          </div>
          <div className="section-title">Thresholds</div>
          <div className="grid g3">
            {THRESHOLDS.map(([k, label]) => (
              <Field key={k} label={label}>
                <input className="input" type="number" min={0} value={p.healthConfig.thresholds[k]} onChange={(e) => setHealth({ thresholds: { ...p.healthConfig.thresholds, [k]: Number(e.target.value) || 0 } })} />
              </Field>
            ))}
          </div>
        </>
      )}
    </Modal>
  );
}

function ProjectSettings() {
  const { project, setProject, toast } = useApp();
  const profiles = useAsync(() => api.profileList(), []);
  const [name, setName] = useState(project.name);
  const [excl, setExcl] = useState(project.config.extraExcludes.join('\n'));
  const [profileId, setProfileId] = useState(project.profileId || '');
  useEffect(() => {
    setName(project.name);
    setExcl(project.config.extraExcludes.join('\n'));
    setProfileId(project.profileId || '');
  }, [project]);
  const save = () =>
    api
      .projectUpdate(project.id, { name, profileId: profileId || null, config: { extraExcludes: excl.split('\n').map((s) => s.trim()).filter(Boolean) }, pinned: null })
      .then((p) => (setProject(p), toast('Project settings saved')), (e) => toast(errorMessage(e), 'error'));
  return (
    <div className="card stack" style={{ maxWidth: 720 }}>
      <Field label="Display name"><input className="input" value={name} onChange={(e) => setName(e.target.value)} /></Field>
      <Field label="Project folder"><div className="path selectable">{project.root}</div></Field>
      <Field label="Default profile for this project">
        <select className="select" value={profileId} onChange={(e) => setProfileId(e.target.value)}>
          <option value="">Use the app default</option>
          {(profiles.data || []).map((p) => <option key={p.id} value={p.id}>{p.name}</option>)}
        </select>
      </Field>
      <Field label="Additional exclusions for this project" help="Added to the profile's exclusions. One glob per line (e.g. fixtures, src/generated)."><textarea className="textarea mono" value={excl} onChange={(e) => setExcl(e.target.value)} /></Field>
      <div><button type="button" className="btn primary" onClick={save}>Save</button></div>
    </div>
  );
}

function Data() {
  const { settings, saveSettings, toast } = useApp();
  const status = useAsync(() => api.dbStatus(), []);
  const backup = async () => {
    const dest = await pickSave('Back up Structura data', `structura-backup-${new Date().toISOString().slice(0, 10)}.db`, ['db'], 'Database');
    if (dest) api.dbBackup(dest).then(() => toast('Backup created'), (e) => toast(errorMessage(e), 'error'));
  };
  const restore = async () => {
    const src = await pickFile('Restore from backup', ['db', 'sqlite', 'bak'], 'Database');
    if (!src) return;
    if (!(await confirm('Replace all Structura data (projects, snapshots, mappings, history, settings) with this backup? A safety copy of the current data is saved first.'))) return;
    api.dbRestore(src).then(() => (toast('Backup restored — restarting'), setTimeout(() => relaunch(), 800)), (e) => toast(errorMessage(e), 'error'));
  };
  return (
    <div className="grid g2">
      <div className="card stack">
        <h3>Database</h3>
        <Async state={status}>
          {(s) => (
            <dl className="kv">
              <dt>Location</dt><dd className="path selectable">{s.path || 'in-memory (degraded mode)'}</dd>
              <dt>Size</dt><dd>{bytes(s.sizeBytes)}</dd>
              <dt>Schema</dt><dd>v{s.schemaVersion} (app supports v{s.latestVersion})</dd>
              <dt>Integrity</dt><dd>{s.integrityOk ? <span className="badge ok">ok</span> : <span className="badge high">problem detected</span>}</dd>
              <dt>Contents</dt><dd>{num(s.projects)} projects · {num(s.snapshots)} snapshots · {num(s.historyEntries)} history entries</dd>
            </dl>
          )}
        </Async>
        <div className="row">
          <button type="button" className="btn" onClick={backup}><Icon name="download" size={14} /> Back up…</button>
          <button type="button" className="btn" onClick={restore}><Icon name="upload" size={14} /> Restore…</button>
        </div>
        <div className="faint xs">Backups are consistent SQLite copies. Restoring validates the file (integrity, schema version) before replacing anything.</div>
      </div>
      <div className="card stack">
        <h3>History retention</h3>
        <Field label="Keep history entries for (days, 0 = forever)">
          <input className="input" type="number" min={0} max={3650} value={settings.historyRetentionDays} onChange={(e) => saveSettings({ ...settings, historyRetentionDays: Math.max(0, Math.min(3650, Number(e.target.value) || 0)) }).catch((x) => toast(errorMessage(x), 'error'))} />
        </Field>
        <div className="faint xs">History stores titles, counts and outcomes only. Snapshots and mappings are never pruned automatically.</div>
      </div>
    </div>
  );
}

function Updates() {
  const { settings, saveSettings, toast } = useApp();
  const [state, setState] = useState(null);
  const [busy, setBusy] = useState(false);
  const check = () => {
    setBusy(true);
    api.updateCheck().then(setState, (e) => toast(errorMessage(e), 'error')).finally(() => setBusy(false));
  };
  const install = async () => {
    if (!(await confirm(`Install Ivily Structura ${state.version}? Your data is backed up first and preserved; the app restarts afterwards.`, 'Install update', 'info'))) return;
    setBusy(true);
    api.updateInstall().then(() => relaunch(), (e) => (toast(errorMessage(e), 'error'), setBusy(false)));
  };
  return (
    <div className="card stack" style={{ maxWidth: 720 }}>
      <h3>Updates</h3>
      <p className="muted small">Updates are downloaded from GitHub Releases and verified with a signature before installation. Checking is the only network request Structura makes, and only when you ask (or on start, if enabled).</p>
      <Check checked={settings.checkUpdates} onChange={(checkUpdates) => saveSettings({ ...settings, checkUpdates })}>Allow update checks</Check>
      <div className="row">
        <button type="button" className="btn" disabled={busy || !settings.checkUpdates} onClick={check}><Icon name="refresh" size={14} /> Check now</button>
        {state?.available && <button type="button" className="btn primary" disabled={busy} onClick={install}><Icon name="download" size={14} /> Install {state.version}</button>}
      </div>
      {state && !state.available && <div className="banner info small"><Icon name="check" /> You are on the latest version ({state.currentVersion}).</div>}
      {state?.available && state.notes && <pre className="md-preview card selectable">{state.notes}</pre>}
    </div>
  );
}

function About() {
  const { info } = useApp();
  if (!info) return null;
  const counts = info.languages.reduce((m, l) => ({ ...m, [l.support]: (m[l.support] || 0) + 1 }), {});
  return (
    <div className="stack">
      <div className="grid g2">
        <div className="card stack">
          <h3>Ivily Structura {info.version}</h3>
          <dl className="kv">
            <dt>Analysis engine</dt><dd>v{info.engineVersion}</dd>
            <dt>Ivily Ecosystem Protocol</dt><dd>IEP {info.iepVersion} (read-only, optional)</dd>
            <dt>Data folder</dt><dd className="path selectable">{info.dataDir}</dd>
            <dt>Licence</dt><dd>MIT</dd>
            <dt>Source</dt><dd className="mono small">github.com/efecrtgl/ivily-structura</dd>
          </dl>
          {info.degraded && <div className="banner danger small">Running in degraded mode: {info.degraded}</div>}
        </div>
        <div className="card stack">
          <h3>Privacy</h3>
          <ul className="muted small" style={{ margin: 0, paddingLeft: 18, lineHeight: 1.7 }}>
            <li>Projects are opened read-only; nothing inside them is modified.</li>
            <li>No telemetry. Source code never leaves this computer.</li>
            <li>Files that usually contain secrets (.env, keys, credentials) are listed but never read.</li>
            <li>Symbolic links are not followed during scans.</li>
            <li>History and snapshots store structure and counts — not file contents.</li>
          </ul>
        </div>
      </div>
      <div className="card flush">
        <div className="card-head" style={{ padding: '14px 16px 0' }}>
          <h3>Language support</h3>
          <span className="hint">{counts.lexical || 0} lexical · {counts.detected || 0} detection-only</span>
        </div>
        <div className="table-wrap" style={{ maxHeight: 420 }}>
          <table className="data">
            <thead><tr><th>Language</th><th>Category</th><th>Extensions</th><th>Analysis level</th></tr></thead>
            <tbody>
              {info.languages.map((l) => (
                <tr key={l.id}>
                  <td>{l.name}</td>
                  <td className="faint">{l.category}</td>
                  <td className="mono faint xs">{l.extensions.map((e) => `.${e}`).join(' ')}</td>
                  <td>{l.support === 'lexical' ? <span className="badge">imports · symbols (lexical)</span> : <span className="badge">detection only</span>}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  );
}
